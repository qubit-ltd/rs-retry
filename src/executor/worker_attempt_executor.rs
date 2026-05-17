/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Single worker-thread attempt execution.

use std::panic;
use std::sync::{
    Arc,
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use crate::{
    AttemptExecutorError,
    AttemptFailure,
    AttemptPanic,
};

const WORKER_DISCONNECTED_MESSAGE: &str = "retry worker thread stopped without sending a result";
const WORKER_SPAWN_FAILED_MESSAGE: &str = "failed to spawn retry worker thread";

/// Builds a spawn-failure attempt outcome at the cold spawn error site.
macro_rules! worker_spawn_failure {
    ($error:expr) => {
        BlockingAttemptOutcome::new(
            Err(AttemptFailure::Executor(
                AttemptExecutorError::with_context(
                    WORKER_SPAWN_FAILED_MESSAGE,
                    &$error.to_string(),
                ),
            )),
            0,
        )
    };
}

/// Runs one blocking attempt on a worker thread.
pub(in crate::executor) struct WorkerAttemptExecutor;

impl WorkerAttemptExecutor {
    /// Runs one blocking attempt on a worker thread.
    ///
    /// # Parameters
    /// - `operation`: Shared blocking operation.
    /// - `attempt_timeout`: Effective timeout for this attempt, if any.
    /// - `worker_cancel_grace`: Maximum time to wait for a timed-out worker after
    ///   cancellation.
    ///
    /// # Returns
    /// The attempt outcome, including the attempt result and unreaped worker count.
    ///
    /// # Worker Behavior
    /// Operation panics are converted into [`AttemptFailure::Panic`]. Worker-spawn
    /// failures are converted into [`AttemptFailure::Executor`].
    pub(in crate::executor) fn run<E>(
        operation: Arc<dyn BlockingAttempt<E>>,
        attempt_timeout: Option<Duration>,
        worker_cancel_grace: Duration,
    ) -> BlockingAttemptOutcome<(), E>
    where
        E: Send + 'static,
    {
        let token = AttemptCancelToken::new();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker_token = token.clone();
        let worker = match std::thread::Builder::new()
            .name("qubit-retry-worker".to_string())
            .spawn(move || {
                let result =
                    panic::catch_unwind(panic::AssertUnwindSafe(|| operation.call(worker_token)));
                let attempt_result = match result {
                    Ok(result) => result,
                    Err(payload) => Err(AttemptFailure::Panic(AttemptPanic::from_payload(payload))),
                };
                let _ = sender.send(attempt_result);
            }) {
            Ok(worker) => worker,
            Err(error) => return worker_spawn_failure!(error),
        };

        match attempt_timeout {
            Some(attempt_timeout) => worker_timeout_result_to_attempt_outcome(
                receiver.recv_timeout(attempt_timeout),
                receiver,
                worker,
                &token,
                worker_cancel_grace,
            ),
            None => worker_recv_result_to_attempt_outcome(receiver.recv(), worker),
        }
    }
}

/// Converts a blocking worker receive result into an attempt outcome.
///
/// # Parameters
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
    let result = result.expect(WORKER_DISCONNECTED_MESSAGE);
    BlockingAttemptOutcome::new(result, 0)
}

/// Converts a timed worker receive result into a blocking attempt outcome.
///
/// # Parameters
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
        token.cancel();
        let worker_exited = wait_for_cancelled_worker(&receiver, worker, worker_cancel_grace);
        let unreaped_worker_count = if worker_exited { 0 } else { 1 };
        BlockingAttemptOutcome::new(Err(AttemptFailure::Timeout), unreaped_worker_count)
    } else {
        join_finished_worker(worker);
        let result = result.expect(WORKER_DISCONNECTED_MESSAGE);
        BlockingAttemptOutcome::new(result, 0)
    }
}

/// Waits briefly for a cancelled worker to exit.
///
/// # Parameters
/// - `receiver`: Worker result receiver used only to observe whether the worker
///   exited.
/// - `worker`: Worker thread handle, joined when exit is observed.
/// - `grace`: Maximum time to wait after cancellation. Zero performs only a
///   non-blocking check.
///
/// # Returns
/// `true` when the worker was observed to exit before the grace period ended,
/// otherwise `false`. When this returns `false`, the worker handle is dropped and
/// the thread may continue running detached.
fn wait_for_cancelled_worker<E>(
    receiver: &mpsc::Receiver<Result<(), AttemptFailure<E>>>,
    worker: JoinHandle<()>,
    grace: Duration,
) -> bool {
    let exited = if grace.is_zero() {
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
/// # Parameters
/// - `worker`: Worker thread handle.
fn join_finished_worker(worker: JoinHandle<()>) {
    let _ = worker.join();
}
