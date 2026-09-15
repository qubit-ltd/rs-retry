// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Single worker-thread attempt execution.
//!
//! This module owns the boundary between retry-flow code and operating-system
//! threads. A runner asks for exactly one attempt outcome; this executor spawns
//! the worker and a detached reaper, captures panics, and waits for both the
//! result and completed join. Cancellation and timeouts remain observable
//! throughout thread-local destruction; an expired grace may leave both
//! threads running, but the calling thread never joins either thread.

#[cfg(test)]
use std::cell::Cell;
use std::future::Future;
use std::io;
use std::panic;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::thread::Builder as ThreadBuilder;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use qubit_clock::TimerFuture;

use super::worker_event::WorkerEvent;
use super::worker_wake::WorkerWake;
use crate::AttemptFailure;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;
use crate::executor::attempt_cancellation_token::AttemptCancellationToken;
use crate::executor::internal::BlockingAttempt;
use crate::executor::internal::BlockingAttemptOutcome;
use crate::executor::retry_cancellation_token::RetryCancellationToken;
use crate::internal::retry_panic_from_payload;

/// Runs one blocking attempt on a worker thread.
pub(in crate::executor) struct WorkerAttemptExecutor;

impl WorkerAttemptExecutor {
    /// Runs one blocking attempt on a worker thread.
    ///
    /// # Parameters
    /// - `operation`: Shared blocking operation.
    /// - `thread_name`: OS-visible worker name.
    /// - `stack_size`: Requested worker stack size, or None for the OS default.
    /// - `cancellation`: Optional external flow cancellation source.
    /// - `prepare`: Callback run after the worker has been spawned but before
    ///   its operation is released. It commits the attempt and returns the
    ///   successful admission. Timer registration precedes admission.
    /// - `timeout`: Registered absolute timeout and its source scope, if
    ///   bounded.
    /// - `worker_cancel_grace`: Maximum time to wait for a timed-out worker
    ///   after cancellation.
    ///
    /// # Returns
    /// The attempt outcome, with worker-spawn and post-timeout cleanup failures
    /// kept separate from operation failures.
    ///
    /// # Worker Behavior
    /// Operation panics are converted into [`AttemptFailure::Panicked`]. The
    /// worker waits behind a start gate until both threads exist, so either
    /// spawn failure is never counted as an operation attempt.
    ///
    /// # Type Parameters
    /// - `E`: Owned application error transferred across the worker boundary.
    /// - `X`: Admission error returned directly to the facade.
    /// - `P`: One-shot callback that commits admission.
    ///
    /// # Errors
    /// Returns the admission callback error without releasing the operation
    /// start gate.
    pub(in crate::executor) fn run<E, X, P>(
        operation: Arc<dyn BlockingAttempt<E>>,
        thread_name: &str,
        stack_size: Option<usize>,
        worker_cancel_grace: Duration,
        cancellation: Option<&RetryCancellationToken>,
        timeout: Option<(RetryTimeoutScope, TimerFuture)>,
        prepare: P,
    ) -> Result<BlockingAttemptOutcome<(), E>, X>
    where
        E: Send + 'static,
        P: FnOnce() -> Result<(), X>,
    {
        // One channel combines completion, cancellation and timer wakeups.
        // Each iteration checks cancellation before timeout readiness, then
        // receives the next queued event. Once the runner drops the receiver,
        // send failure only means the retry flow has already terminated.
        let token = AttemptCancellationToken::new();
        let (sender, receiver) = mpsc::channel();
        let sender = Arc::new(sender);
        let (start_sender, start_receiver) = mpsc::sync_channel(0);
        let worker_token = token.clone();
        let worker_sender = sender.clone();
        let mut builder = ThreadBuilder::new().name(thread_name.to_owned());
        if let Some(stack_size) = stack_size {
            builder = builder.stack_size(stack_size);
        }
        let worker = match builder.spawn(move || {
            if start_receiver.recv().is_err() {
                return;
            }
            // Worker mode is the only synchronous mode with a panic
            // isolation boundary. Convert panic payloads into retry
            // failures so policy and listeners can handle them normally.
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                operation.call(worker_token)
            }));
            let attempt_result = match result {
                Ok(result) => result,
                Err(payload) => Err(AttemptFailure::Panicked {
                    panic: retry_panic_from_payload(payload),
                }),
            };
            let _ = worker_sender.send(WorkerEvent::Completed(attempt_result));
        }) {
            Ok(worker) => worker,
            Err(error) => {
                return Ok(BlockingAttemptOutcome::WorkerSpawnFailed {
                    message: error.to_string().into_boxed_str(),
                });
            }
        };

        // Transfer the only join handle before admission. A failed reaper
        // spawn drops its closure and handle; closing the start gate lets the
        // unadmitted worker exit without ever calling user code.
        if let Err(error) = spawn_reaper(worker, Arc::clone(&sender)) {
            drop(start_sender);
            return Ok(BlockingAttemptOutcome::WorkerSpawnFailed {
                message: format!("reaper: {error}").into_boxed_str(),
            });
        }

        let mut cancellation_future =
            cancellation.map(|token| Box::pin(token.cancelled()));
        if let Some(future) = cancellation_future.as_mut() {
            register_cancellation_waker(future, &sender);
        }

        match prepare() {
            Ok(()) => {}
            Err(error) => {
                drop(start_sender);
                return Err(error);
            }
        };
        if start_sender.send(()).is_err() {
            return Ok(BlockingAttemptOutcome::WorkerChannelClosed);
        }

        // Wakers must not keep a broken protocol alive by retaining a sender.
        let waker = Waker::from(Arc::new(WorkerWake {
            sender: Arc::downgrade(&sender),
        }));
        drop(sender);
        Ok(wait_for_worker(
            receiver,
            &token,
            worker_cancel_grace,
            cancellation,
            timeout,
            &waker,
        ))
    }
}

/// Waits for both the operation result and proof of thread exit while keeping
/// the original cancellation and absolute timeout active.
///
/// # Type Parameters
/// - `E`: Owned application error received from the worker.
///
/// # Parameters
/// - `receiver`: Channel carrying operation results, join proof, and readiness.
/// - `token`: Attempt token marked when a stop event wins.
/// - `grace`: Real-time cleanup bound after cancellation.
/// - `cancellation`: Optional flow cancellation source.
/// - `timeout`: Optional registered deadline paired with its scope.
/// - `waker`: Notification waker for the timer future.
///
/// # Returns
/// Completion only after both result and join; cancellation wins timeout,
/// and timeout wins queued completion when they are observed together.
fn wait_for_worker<E: Send + 'static>(
    receiver: mpsc::Receiver<WorkerEvent<E>>,
    token: &AttemptCancellationToken,
    grace: Duration,
    cancellation: Option<&RetryCancellationToken>,
    mut timeout: Option<(RetryTimeoutScope, TimerFuture)>,
    waker: &Waker,
) -> BlockingAttemptOutcome<(), E> {
    let mut context = Context::from_waker(waker);
    let mut completed = None;
    let mut joined = false;
    loop {
        if cancellation.is_some_and(RetryCancellationToken::is_cancelled) {
            return stop_worker(
                &receiver,
                token,
                grace,
                WorkerStopTrigger::Cancellation,
                joined,
            );
        }
        if let Some((scope, future)) = timeout.as_mut()
            && let Poll::Ready(result) = future.as_mut().poll(&mut context)
        {
            match result {
                Ok(()) => {
                    return stop_worker(
                        &receiver,
                        token,
                        grace,
                        timeout_trigger(*scope),
                        joined,
                    );
                }
                Err(error) => {
                    token.cancel();
                    return match observe_worker_exit(&receiver, grace, joined) {
                        Ok(true) => {
                            BlockingAttemptOutcome::TimerFailed { error }
                        }
                        Ok(false) => {
                            BlockingAttemptOutcome::WorkerStillRunning {
                                trigger: WorkerStopTrigger::TimerFailure,
                            }
                        }
                        Err(_) => BlockingAttemptOutcome::WorkerChannelClosed,
                    };
                }
            }
        }
        if joined && let Some(result) = completed.take() {
            return BlockingAttemptOutcome::Completed(result);
        }
        match receiver.recv() {
            Ok(WorkerEvent::Completed(result)) => completed = Some(result),
            Ok(WorkerEvent::Joined(Ok(()))) => joined = true,
            Ok(WorkerEvent::Joined(Err(panic))) => {
                // A failed join may have no Completed event. Its panic is the
                // attempt result, but observed cancellation still takes
                // priority.
                joined = true;
                completed = Some(Err(AttemptFailure::Panicked { panic }));
            }
            Ok(WorkerEvent::Wake) => {}
            Err(_) => return BlockingAttemptOutcome::WorkerChannelClosed,
        }
    }
}

/// Starts the detached reaper, which exclusively owns the blocking join.
/// A spawn error leaves the worker unadmitted behind its start gate.
///
/// # Type Parameters
/// - `E`: Application error carried by the event channel.
///
/// # Parameters
/// - `worker`: Exclusive join handle transferred to the reaper.
/// - `sender`: Channel retaining the eventual join proof.
///
/// # Returns
/// Unit after the reaper starts; its join runs independently.
///
/// # Errors
/// Returns the operating-system thread-spawn error.
fn spawn_reaper<E: Send + 'static>(
    worker: JoinHandle<()>,
    sender: Arc<mpsc::Sender<WorkerEvent<E>>>,
) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_REAPER_SPAWN.with(|fail| fail.replace(false)) {
        return Err(io::Error::other("injected reaper spawn failure"));
    }
    ThreadBuilder::new()
        .name("retry-reaper".into())
        .spawn(move || {
            let joined = worker.join().map_err(retry_panic_from_payload);
            let _ = sender.send(WorkerEvent::Joined(joined));
        })?;
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAIL_REAPER_SPAWN: Cell<bool> = const { Cell::new(false) };
}

/// Polls a cancellation future once with an event-channel waker.
///
/// # Parameters
/// - `future`: Cancellation future to register.
/// - `sender`: Worker event sender weakly referenced by the waker.
///
/// # Type Parameters
/// - `E`: Application error carried by worker events.
#[inline]
fn register_cancellation_waker<E: Send + 'static>(
    future: &mut Pin<Box<crate::RetryCancelled<'_>>>,
    sender: &Arc<mpsc::Sender<WorkerEvent<E>>>,
) {
    let waker = Waker::from(Arc::new(WorkerWake {
        sender: Arc::downgrade(sender),
    }));
    let mut context = Context::from_waker(&waker);
    if future.as_mut().poll(&mut context).is_ready() {
        let _ = sender.send(WorkerEvent::Wake);
    }
}

/// Maps an effective timeout scope to the worker stop trigger.
///
/// # Parameters
/// - `scope`: Effective timeout scope selected by the retry controller.
///
/// # Returns
/// The corresponding stable worker stop trigger.
#[inline]
#[must_use = "use the prepared value or inspect the result"]
fn timeout_trigger(scope: RetryTimeoutScope) -> WorkerStopTrigger {
    match scope {
        RetryTimeoutScope::Attempt => WorkerStopTrigger::AttemptTimeout,
        RetryTimeoutScope::Flow => WorkerStopTrigger::FlowTimeout,
    }
}

/// Cancels after a fixed trigger and waits only for proof of thread exit.
/// Channel loss terminates the flow as an infrastructure failure.
///
/// # Type Parameters
/// - `E`: Application error carried by worker events.
///
/// # Parameters
/// - `receiver`: Channel used only to observe worker exit.
/// - `token`: Attempt token cancelled before waiting.
/// - `grace`: Real-time bound that does not reset on unrelated events.
/// - `trigger`: First event that requested stopping.
/// - `joined`: Whether thread exit has already been proven.
///
/// # Returns
/// Stopped with the original trigger, still-running on expiry, or
/// channel-closed status.
fn stop_worker<E>(
    receiver: &mpsc::Receiver<WorkerEvent<E>>,
    token: &AttemptCancellationToken,
    grace: Duration,
    trigger: WorkerStopTrigger,
    joined: bool,
) -> BlockingAttemptOutcome<(), E> {
    token.cancel();
    match observe_worker_exit(receiver, grace, joined) {
        Ok(true) => BlockingAttemptOutcome::Stopped { trigger },
        Ok(false) => BlockingAttemptOutcome::WorkerStillRunning { trigger },
        Err(_) => BlockingAttemptOutcome::WorkerChannelClosed,
    }
}

/// Observes Joined without allowing unrelated events to reset grace.
/// Returns true only for a joined worker, false on expiry, and Err for a
/// broken channel. An unrepresentable Instant deadline means unbounded grace.
///
/// # Type Parameters
/// - `E`: Application error carried by events discarded during cleanup.
///
/// # Parameters
/// - `receiver`: Event channel supplying join proof.
/// - `grace`: Real-time bound; an unrepresentable deadline waits without a
///   bound.
/// - `joined`: Prior proof of worker exit.
///
/// # Returns
/// True if joined, false if grace expires before join proof.
///
/// # Errors
/// Returns channel disconnection if no join proof was received.
fn observe_worker_exit<E>(
    receiver: &mpsc::Receiver<WorkerEvent<E>>,
    grace: Duration,
    joined: bool,
) -> Result<bool, mpsc::RecvError> {
    if joined {
        return Ok(true);
    }
    let deadline = Instant::now().checked_add(grace);
    loop {
        let event = if grace.is_zero() {
            match receiver.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => return Ok(false),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(mpsc::RecvError);
                }
            }
        } else if let Some(deadline) = deadline {
            let Some(remaining) =
                deadline.checked_duration_since(Instant::now())
            else {
                return Ok(false);
            };
            match receiver.recv_timeout(remaining) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(false),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(mpsc::RecvError);
                }
            }
        } else {
            receiver.recv()?
        };
        if matches!(event, WorkerEvent::Joined(_)) {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::task::Waker;
    use std::time::Duration;

    use super::FAIL_REAPER_SPAWN;
    use super::WorkerEvent;
    use super::WorkerWake;
    use super::observe_worker_exit;
    use super::stop_worker;
    use super::wait_for_worker;
    use crate::AttemptCancellationToken;
    use crate::AttemptFailure;
    use crate::RetryCancellationToken;
    use crate::RetryConfig;
    use crate::RetryErrorReason;
    use crate::RetryInfrastructureFailure;
    use crate::RetryPanic;
    use crate::RetryPolicy;
    use crate::RetryTimeoutScope;
    use crate::WorkerRetry;
    use crate::WorkerStopTrigger;
    use crate::executor::internal::BlockingAttemptOutcome;

    /// Exercises private protocol ordering without relying on OS scheduling.
    #[test]
    fn test_worker_events_accept_both_completion_orders() {
        for joined_first in [false, true] {
            let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
            let completed = WorkerEvent::Completed(Ok(()));
            let joined = WorkerEvent::Joined(Ok(()));
            let events = if joined_first {
                [joined, completed]
            } else {
                [completed, joined]
            };
            for event in events {
                sender.send(event).expect("receiver alive");
            }
            drop(sender);
            let result = wait_for_worker(
                receiver,
                &AttemptCancellationToken::new(),
                Duration::ZERO,
                None,
                None,
                Waker::noop(),
            );
            assert!(matches!(
                result,
                BlockingAttemptOutcome::Completed(Ok(()))
            ));
        }
    }

    /// Ready controls outrank a queued result and join in either arrival order.
    #[test]
    fn test_worker_ready_cancellation_and_timeout_precede_completed_join() {
        for cancelled in [false, true] {
            for joined_first in [false, true] {
                let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
                let completed = WorkerEvent::Completed(Ok(()));
                let joined = WorkerEvent::Joined(Ok(()));
                for event in if joined_first {
                    [joined, completed]
                } else {
                    [completed, joined]
                } {
                    sender.send(event).expect("receiver alive");
                }
                let flow_token = RetryCancellationToken::new();
                if cancelled {
                    flow_token.cancel();
                }
                let attempt_token = AttemptCancellationToken::new();
                let outcome = wait_for_worker(
                    receiver,
                    &attempt_token,
                    Duration::ZERO,
                    Some(&flow_token),
                    Some((
                        RetryTimeoutScope::Attempt,
                        Box::pin(async { Ok(()) }),
                    )),
                    Waker::noop(),
                );
                let expected = if cancelled {
                    WorkerStopTrigger::Cancellation
                } else {
                    WorkerStopTrigger::AttemptTimeout
                };
                assert!(
                    matches!(outcome, BlockingAttemptOutcome::Stopped { trigger } if trigger == expected)
                );
                assert!(attempt_token.is_cancelled());
            }
        }
    }

    /// A join panic supplies its own result when no Completed event exists.
    #[test]
    fn test_worker_join_panic_needs_no_completed_event() {
        let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
        sender
            .send(WorkerEvent::Joined(Err(RetryPanic::StaticStr(
                "join panic",
            ))))
            .expect("receiver alive");
        drop(sender);
        let result = wait_for_worker(
            receiver,
            &AttemptCancellationToken::new(),
            Duration::ZERO,
            None,
            None,
            Waker::noop(),
        );
        assert!(matches!(
            result,
            BlockingAttemptOutcome::Completed(Err(AttemptFailure::Panicked {
                panic: RetryPanic::StaticStr("join panic")
            }))
        ));
    }

    /// Neither a partial result nor an exit marker makes an incomplete protocol
    /// valid.
    #[test]
    fn test_worker_channel_closure_terminates_incomplete_protocol() {
        for event in [
            WorkerEvent::Completed(Ok(())),
            WorkerEvent::Joined(Ok(())),
            WorkerEvent::Wake,
        ] {
            let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
            let sender = Arc::new(sender);
            let waker = Waker::from(Arc::new(WorkerWake {
                sender: Arc::downgrade(&sender),
            }));
            sender.send(event).expect("receiver alive");
            drop(sender);
            waker.wake_by_ref();
            let result = wait_for_worker(
                receiver,
                &AttemptCancellationToken::new(),
                Duration::ZERO,
                None,
                None,
                &waker,
            );
            assert!(matches!(
                result,
                BlockingAttemptOutcome::WorkerChannelClosed
            ));
        }
    }

    /// Grace consumes the same events, but only Joined proves thread exit.
    #[test]
    fn test_worker_grace_ignores_completed_and_wake() {
        let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
        sender
            .send(WorkerEvent::Completed(Ok(())))
            .expect("receiver alive");
        sender.send(WorkerEvent::Wake).expect("receiver alive");
        assert!(
            !observe_worker_exit(&receiver, Duration::ZERO, false)
                .expect("channel open")
        );
        sender
            .send(WorkerEvent::Joined(Ok(())))
            .expect("receiver alive");
        assert!(
            observe_worker_exit(&receiver, Duration::ZERO, false)
                .expect("channel open")
        );
    }

    /// Disconnection during any grace mode is infrastructure failure, never
    /// exit proof.
    #[test]
    fn test_worker_grace_channel_closure_is_not_exit() {
        for grace in [Duration::ZERO, Duration::from_secs(1), Duration::MAX] {
            let (sender, receiver) = mpsc::channel::<WorkerEvent<()>>();
            drop(sender);
            let token = AttemptCancellationToken::new();
            let result = stop_worker(
                &receiver,
                &token,
                grace,
                WorkerStopTrigger::Cancellation,
                false,
            );
            assert!(matches!(
                result,
                BlockingAttemptOutcome::WorkerChannelClosed
            ));
            assert!(token.is_cancelled());
        }
    }

    /// The private seam fails reaper creation before admission or user code.
    #[test]
    fn test_worker_reaper_spawn_failure_has_zero_attempts() {
        FAIL_REAPER_SPAWN.with(|fail| fail.set(true));
        let config = RetryConfig::<()>::builder()
            .policy(RetryPolicy::builder().build().expect("valid policy"))
            .build()
            .expect("valid config");
        let error = WorkerRetry::new(&config)
            .run(|_| -> Result<(), ()> {
                panic!("operation must remain behind gate")
            })
            .expect_err("reaper spawn fails");
        assert_eq!(error.context().attempts(), 0);
        assert_eq!(error.context().current_attempt(), None);
        assert!(matches!(
            error.reason(),
            RetryErrorReason::Infrastructure { failure: RetryInfrastructureFailure::WorkerSpawn { message } }
                if message.contains("reaper")
        ));
    }
}
