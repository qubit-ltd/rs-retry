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

use std::panic;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use crate::AttemptFailure;
use crate::RetryPanic;

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
        prepare: P,
    ) -> Result<BlockingAttemptOutcome<(), E>, X>
    where
        E: Send + 'static,
        P: FnOnce() -> Result<Option<Duration>, X>,
    {
        // A bounded channel is enough because each worker sends exactly one
        // final attempt result. If the runner times out and drops the receiver,
        // send failure only means the retry flow has already moved on.
        let token = AttemptCancelToken::new();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (start_sender, start_receiver) = mpsc::sync_channel(0);
        let worker_token = token.clone();
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
            let _ = sender.send(attempt_result);
        }) {
            Ok(worker) => worker,
            Err(error) => {
                return Ok(BlockingAttemptOutcome::WorkerSpawnFailed {
                    message: error.to_string().into_boxed_str(),
                });
            }
        };

        let attempt_timeout = match prepare() {
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

        let outcome = match attempt_timeout {
            Some(attempt_timeout) => worker_timeout_result_to_attempt_outcome(
                receiver.recv_timeout(attempt_timeout),
                receiver,
                worker,
                &token,
                worker_cancel_grace,
            ),
            None => {
                worker_recv_result_to_attempt_outcome(receiver.recv(), worker)
            }
        };
        Ok(outcome)
    }
}

/// Converts a blocking worker receive result into an attempt outcome.
///
/// # Arguments
/// - `result`: Result from waiting for the worker without an attempt timeout.
/// - `worker`: Worker thread handle for joining the finished worker.
///
/// # Returns
/// The attempt outcome. The worker is expected to send exactly one result even
/// when the operation panics.
fn worker_recv_result_to_attempt_outcome<E>(
    result: Result<Result<(), AttemptFailure<E>>, mpsc::RecvError>,
    worker: JoinHandle<()>,
) -> BlockingAttemptOutcome<(), E> {
    join_finished_worker(worker);
    let result = result.expect("worker thread must send exactly one result");
    BlockingAttemptOutcome::Completed(result)
}

/// Converts a timed worker receive result into a blocking attempt outcome.
///
/// # Arguments
/// - `result`: Result from waiting for the worker up to the attempt timeout.
/// - `receiver`: Receiver used for the post-timeout cancellation grace wait.
/// - `worker`: Worker thread handle for joining finished workers.
/// - `token`: Cancellation token to mark when the receive timed out.
/// - `worker_cancel_grace`: Maximum time to wait for a timed-out worker after
///   cancellation.
///
/// # Returns
/// The attempt outcome, including unreaped-worker accounting for timeout cases.
fn worker_timeout_result_to_attempt_outcome<E>(
    result: Result<Result<(), AttemptFailure<E>>, mpsc::RecvTimeoutError>,
    receiver: mpsc::Receiver<Result<(), AttemptFailure<E>>>,
    worker: JoinHandle<()>,
    token: &AttemptCancelToken,
    worker_cancel_grace: Duration,
) -> BlockingAttemptOutcome<(), E>
where
    E: Send + 'static,
{
    if let Err(mpsc::RecvTimeoutError::Timeout) = result {
        // Rust cannot forcibly stop a thread. The timeout marks the cooperative
        // token first, then waits briefly so well-behaved operations can return
        // and be joined before retry policy decides what to do next.
        token.cancel();
        let worker_exited =
            wait_for_cancelled_worker(&receiver, worker, worker_cancel_grace);
        if worker_exited {
            BlockingAttemptOutcome::TimedOut
        } else {
            BlockingAttemptOutcome::WorkerStillRunning
        }
    } else {
        join_finished_worker(worker);
        let result =
            result.expect("worker thread must send exactly one result");
        BlockingAttemptOutcome::Completed(result)
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
fn wait_for_cancelled_worker<E>(
    receiver: &mpsc::Receiver<Result<(), AttemptFailure<E>>>,
    worker: JoinHandle<()>,
    grace: Duration,
) -> bool {
    let exited = if grace.is_zero() {
        // Zero grace still checks once so already-finished workers are joined
        // instead of being reported as detached unnecessarily.
        !matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty))
    } else {
        match receiver.recv_timeout(grace) {
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => false,
        }
    };
    if exited {
        join_finished_worker(worker);
    }
    exited
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
