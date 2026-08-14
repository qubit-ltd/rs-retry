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
//! the worker, captures panics, waits for the result or timeout, requests
//! cooperative cancellation, and reports whether a timed-out worker could not
//! be reaped during the grace period.

use std::future::Future;
use std::panic;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc;
use std::task::Context;
use std::task::Wake;
use std::task::Waker;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use super::attempt_cancellation_token::AttemptCancellationToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use super::internal::EffectiveTimeout;
use super::retry_cancellation_token::RetryCancellationToken;
use crate::AttemptFailure;
use crate::RetryPanic;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;

/// Event observed while waiting for one worker attempt.
enum WorkerEvent<E> {
    /// The worker finished and produced an attempt result.
    Completed(Result<(), AttemptFailure<E>>),
    /// The retry-flow cancellation token was cancelled.
    Cancellation,
}

/// Waker that forwards flow cancellation into the worker event channel.
struct CancellationWake<E> {
    /// Event sender owned by the registered cancellation future.
    sender: mpsc::Sender<WorkerEvent<E>>,
}

impl<E: Send + 'static> Wake for CancellationWake<E> {
    /// Sends one cancellation event when the registered future is woken.
    fn wake(self: Arc<Self>) {
        let _ = self.sender.send(WorkerEvent::Cancellation);
    }

    /// Sends one cancellation event without consuming the shared waker.
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.sender.send(WorkerEvent::Cancellation);
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
    ///   effective timeout.
    /// - `worker_cancel_grace`: Maximum time to wait for a timed-out worker
    ///   after cancellation.
    ///
    /// # Returns
    /// The attempt outcome, with worker-spawn and post-timeout cleanup failures
    /// kept separate from operation failures.
    ///
    /// # Worker Behavior
    /// Operation panics are converted into [`AttemptFailure::Panicked`]. The
    /// worker waits behind a start gate so a spawn failure is never counted as
    /// an operation attempt.
    pub(in crate::executor) fn run<E, X, P>(
        operation: Arc<dyn BlockingAttempt<E>>,
        thread_name: &str,
        stack_size: Option<usize>,
        worker_cancel_grace: Duration,
        cancellation: Option<&RetryCancellationToken>,
        prepare: P,
    ) -> Result<BlockingAttemptOutcome<(), E>, X>
    where
        E: Send + 'static,
        P: FnOnce() -> Result<Option<EffectiveTimeout>, X>,
    {
        // One channel combines the worker result and flow-cancellation wakeup
        // so whichever event is received first fixes the stop decision. If the
        // runner stops and drops the receiver, send failure only means the
        // retry flow has already terminated.
        let token = AttemptCancellationToken::new();
        let (sender, receiver) = mpsc::channel();
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

        let mut cancellation_future =
            cancellation.map(|token| Box::pin(token.cancelled()));
        if let Some(future) = cancellation_future.as_mut() {
            register_cancellation_waker(future, &sender);
        }

        let effective_timeout = match prepare() {
            Ok(timeout) => timeout,
            Err(error) => {
                drop(start_sender);
                join_finished_worker(worker);
                return Err(error);
            }
        };
        start_sender
            .send(())
            .expect("a spawned worker must wait for its start signal");

        let first_event = match effective_timeout {
            Some(timeout) => match receiver.recv_timeout(timeout.duration()) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let trigger = if cancellation
                        .is_some_and(RetryCancellationToken::is_cancelled)
                    {
                        WorkerStopTrigger::Cancellation
                    } else {
                        timeout_trigger(timeout.scope())
                    };
                    return Ok(stop_worker(
                        receiver,
                        worker,
                        &token,
                        worker_cancel_grace,
                        trigger,
                    ));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("worker event channel disconnected unexpectedly")
                }
            },
            None => receiver
                .recv()
                .expect("worker event channel must produce one event"),
        };
        let outcome = match first_event {
            WorkerEvent::Completed(result) => {
                if cancellation
                    .is_some_and(RetryCancellationToken::is_cancelled)
                {
                    token.cancel();
                    join_finished_worker(worker);
                    BlockingAttemptOutcome::Stopped {
                        trigger: WorkerStopTrigger::Cancellation,
                    }
                } else {
                    join_finished_worker(worker);
                    BlockingAttemptOutcome::Completed(result)
                }
            }
            WorkerEvent::Cancellation => stop_worker(
                receiver,
                worker,
                &token,
                worker_cancel_grace,
                WorkerStopTrigger::Cancellation,
            ),
        };
        Ok(outcome)
    }
}

/// Polls a cancellation future once with an event-channel waker.
///
/// # Arguments
/// - `future`: Cancellation future to register.
/// - `sender`: Worker event sender cloned into the waker.
fn register_cancellation_waker<E: Send + 'static>(
    future: &mut Pin<Box<super::retry_cancellation_token::RetryCancelled<'_>>>,
    sender: &mpsc::Sender<WorkerEvent<E>>,
) {
    let waker = Waker::from(Arc::new(CancellationWake {
        sender: sender.clone(),
    }));
    let mut context = Context::from_waker(&waker);
    if future.as_mut().poll(&mut context).is_ready() {
        let _ = sender.send(WorkerEvent::Cancellation);
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

/// Cancels a worker after one fixed stop trigger and waits for its exit.
///
/// # Arguments
/// - `receiver`: Event receiver used to observe worker completion.
/// - `worker`: Worker handle joined only after completion is observed.
/// - `token`: Attempt token marked before the grace wait begins.
/// - `worker_cancel_grace`: Maximum cooperative cancellation grace period.
/// - `trigger`: First event that requested the worker to stop.
///
/// # Returns
/// A stopped outcome retaining the supplied trigger and whether the worker was
/// reaped during its grace period.
fn stop_worker<E>(
    receiver: mpsc::Receiver<WorkerEvent<E>>,
    worker: JoinHandle<()>,
    token: &AttemptCancellationToken,
    worker_cancel_grace: Duration,
    trigger: WorkerStopTrigger,
) -> BlockingAttemptOutcome<(), E>
where
    E: Send + 'static,
{
    token.cancel();
    let worker_exited =
        wait_for_stopped_worker(&receiver, worker, worker_cancel_grace);
    if worker_exited {
        BlockingAttemptOutcome::Stopped { trigger }
    } else {
        BlockingAttemptOutcome::WorkerStillRunning { trigger }
    }
}

/// Waits briefly for a cancelled worker to exit.
///
/// # Arguments
/// - `receiver`: Worker result receiver used only to observe whether the worker
///   exited.
/// - `worker`: Worker thread handle, joined when exit is observed.
/// - `grace`: Maximum time to wait after cancellation. Zero performs only a
///   non-blocking check.
///
/// # Returns
/// `true` when the worker was observed to exit before the grace period ended,
/// otherwise `false`. When this returns `false`, the worker handle is dropped
/// and the thread may continue running detached.
fn wait_for_stopped_worker<E>(
    receiver: &mpsc::Receiver<WorkerEvent<E>>,
    worker: JoinHandle<()>,
    grace: Duration,
) -> bool {
    let exited = observe_worker_exit(receiver, grace);
    if exited {
        join_finished_worker(worker);
    }
    exited
}

/// Observes worker completion without allowing unrelated events to reset grace.
///
/// # Arguments
/// - `receiver`: Worker event receiver.
/// - `grace`: Maximum total time to wait.
///
/// # Returns
/// `true` after a completion or channel disconnection, otherwise `false` when
/// the fixed grace deadline expires. A grace too large for [`Instant`] is
/// treated as unbounded and waits until completion or disconnection; unrelated
/// cancellation events never reset a representable deadline.
fn observe_worker_exit<E>(
    receiver: &mpsc::Receiver<WorkerEvent<E>>,
    grace: Duration,
) -> bool {
    let deadline = Instant::now().checked_add(grace);
    loop {
        let event = if grace.is_zero() {
            match receiver.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => return true,
            }
        } else if let Some(deadline) = deadline {
            let Some(remaining) =
                deadline.checked_duration_since(Instant::now())
            else {
                return false;
            };
            match receiver.recv_timeout(remaining) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => return false,
                Err(mpsc::RecvTimeoutError::Disconnected) => return true,
            }
        } else {
            match receiver.recv() {
                Ok(event) => event,
                Err(mpsc::RecvError) => return true,
            }
        };
        if matches!(event, WorkerEvent::Completed(_)) {
            return true;
        }
    }
}

/// Joins a worker thread that has already been observed to finish.
///
/// # Arguments
/// - `worker`: Worker thread handle.
fn join_finished_worker(worker: JoinHandle<()>) {
    let _ = worker.join();
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
