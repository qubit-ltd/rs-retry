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

use std::future::Future;
use std::panic;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Weak;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use qubit_clock::TimerFuture;

use super::attempt_cancellation_token::AttemptCancellationToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use super::retry_cancellation_token::RetryCancellationToken;
use crate::AttemptFailure;
use crate::RetryPanic;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;

/// Event observed while waiting for one worker attempt.
enum WorkerEvent<E> {
    /// The operation returned; thread-local destruction may still be running.
    Completed(Result<(), AttemptFailure<E>>),
    /// The reaper joined the worker, including thread-local destruction.
    Joined(Result<(), RetryPanic>),
    /// A timer or cancellation future needs another poll.
    Wake,
}

/// Waker that forwards cancellation and timer readiness into the worker event
/// channel.
struct WorkerWake<E> {
    /// Weak sender that cannot conceal loss of both worker and reaper.
    sender: Weak<mpsc::Sender<WorkerEvent<E>>>,
}

impl<E: Send + 'static> Wake for WorkerWake<E> {
    /// Sends a readiness notification when a registered future is woken.
    fn wake(self: Arc<Self>) {
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(WorkerEvent::Wake);
        }
    }

    /// Sends readiness without consuming the shared waker.
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(WorkerEvent::Wake);
        }
    }
}

/// Runs one blocking attempt on a worker thread.
pub(in crate::executor) struct WorkerAttemptExecutor;

impl WorkerAttemptExecutor {
    /// Runs one blocking attempt on a worker thread.
    ///
    /// # Arguments
    /// - `operation`: Shared blocking operation.
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
        let mut builder =
            std::thread::Builder::new().name(thread_name.to_owned());
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
                    panic: retry_panic(payload),
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
                // attempt result, but observed cancellation still takes priority.
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
fn spawn_reaper<E: Send + 'static>(
    worker: JoinHandle<()>,
    sender: Arc<mpsc::Sender<WorkerEvent<E>>>,
) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_REAPER_SPAWN.with(|fail| fail.replace(false)) {
        return Err(std::io::Error::other("injected reaper spawn failure"));
    }
    std::thread::Builder::new()
        .name("retry-reaper".into())
        .spawn(move || {
            let joined = worker.join().map_err(retry_panic);
            let _ = sender.send(WorkerEvent::Joined(joined));
        })?;
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAIL_REAPER_SPAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Polls a cancellation future once with an event-channel waker.
///
/// # Arguments
/// - `future`: Cancellation future to register.
/// - `sender`: Worker event sender weakly referenced by the waker.
fn register_cancellation_waker<E: Send + 'static>(
    future: &mut Pin<Box<super::RetryCancelled<'_>>>,
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
/// # Arguments
/// - `scope`: Effective timeout scope selected by the retry controller.
///
/// # Returns
/// The corresponding stable worker stop trigger.
fn timeout_trigger(scope: RetryTimeoutScope) -> WorkerStopTrigger {
    match scope {
        RetryTimeoutScope::Attempt => WorkerStopTrigger::AttemptTimeout,
        RetryTimeoutScope::Flow => WorkerStopTrigger::FlowTimeout,
    }
}

/// Cancels after a fixed trigger and waits only for proof of thread exit.
/// Channel loss terminates the flow as an infrastructure failure.
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

/// Converts a dynamically typed panic payload into its stable public form.
fn retry_panic(payload: Box<dyn std::any::Any + Send>) -> RetryPanic {
    match payload.downcast::<&'static str>() {
        Ok(message) => RetryPanic::StaticStr(*message),
        Err(payload) => match payload.downcast::<String>() {
            Ok(message) => RetryPanic::String(*message),
            Err(_) => RetryPanic::NonString,
        },
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
    use crate::Retry;
    use crate::RetryFailure;
    use crate::RetryInfrastructureFailure;
    use crate::RetryPanic;
    use crate::RetryPolicy;
    use crate::WorkerStopTrigger;
    use crate::executor::blocking_attempt_outcome::BlockingAttemptOutcome;

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

    /// Neither a partial result nor an exit marker makes an incomplete protocol valid.
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

    /// Disconnection during any grace mode is infrastructure failure, never exit proof.
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
        let error = Retry::<()>::builder(
            RetryPolicy::builder().build().expect("valid policy"),
        )
        .build()
        .worker()
        .run(|_| -> Result<(), ()> {
            panic!("operation must remain behind gate")
        })
        .expect_err("reaper spawn fails");
        assert_eq!(error.context().attempts(), 0);
        assert_eq!(error.context().current_attempt(), None);
        assert!(matches!(error.failure(), RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerSpawn { message }, last_failure: None, ..
        } if message.contains("reaper")));
    }
}
