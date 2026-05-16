/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Worker-thread retry runner.

use std::panic;
use std::sync::{
    Arc,
    mpsc,
};
use std::thread::JoinHandle;
use std::time::{
    Duration,
    Instant,
};

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use super::blocking_value_operation::BlockingValueOperation;
use super::retry::{
    Retry,
    sleep_blocking,
};
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::{
    AttemptExecutorError,
    AttemptFailure,
    AttemptPanic,
    RetryError,
    RetryErrorReason,
};

const WORKER_DISCONNECTED_MESSAGE: &str = "retry worker thread stopped without sending a result";
const WORKER_SPAWN_FAILED_MESSAGE: &str = "failed to spawn retry worker thread";

/// Builds an executor failure for a disconnected worker.
macro_rules! worker_disconnected_failure {
    () => {
        Err(AttemptFailure::Executor(AttemptExecutorError::new(
            WORKER_DISCONNECTED_MESSAGE,
        )))
    };
}

/// Builds an attempt outcome for a worker-spawn failure.
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

/// Blocking worker-thread execution for retry policies.
#[allow(clippy::result_large_err)]
impl<E> Retry<E> {
    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// Each attempt runs on a worker thread. Worker panics are captured as
    /// [`crate::AttemptFailure::Panic`]. Worker-spawn failures are reported as
    /// [`crate::AttemptFailure::Executor`]. If the effective timeout expires,
    /// the retry executor stops waiting and marks the attempt's
    /// [`AttemptCancelToken`] as cancelled. It then waits up to
    /// [`crate::RetryOptions::worker_cancel_grace`] for the worker to exit.
    /// Configured attempt-timeout expirations continue according to
    /// [`crate::AttemptTimeoutPolicy`] only when the worker exits within that
    /// grace period; otherwise the retry flow stops with
    /// [`RetryErrorReason::WorkerStillRunning`]. Elapsed-budget expirations stop
    /// with [`RetryErrorReason::MaxOperationElapsedExceeded`] or
    /// [`RetryErrorReason::MaxTotalElapsedExceeded`].
    ///
    /// # Parameters
    /// - `operation`: Thread-safe operation called once per attempt. It receives
    ///   a cooperative cancellation token for that attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Does not propagate operation panics. Listener panic behavior follows this
    /// retry policy's listener isolation setting.
    ///
    /// # Blocking
    /// Blocks the current thread while waiting for each worker result or timeout
    /// and while sleeping between retry attempts.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time. Worker attempts use
    /// the shortest of configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget as
    /// their effective timeout.
    pub fn run_in_worker<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let operation = Arc::new(BlockingValueOperation::new(operation));
        let worker_operation: Arc<dyn BlockingAttempt<E>> = operation.clone();
        self.run_worker_operation(worker_operation)
            .map(|()| operation.take_value())
    }

    /// Runs a type-erased blocking operation with retry inside worker-thread attempts.
    ///
    /// # Parameters
    /// - `operation`: Shared type-erased operation called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    fn run_worker_operation(
        &self,
        operation: Arc<dyn BlockingAttempt<E>>,
    ) -> Result<(), RetryError<E>>
    where
        E: Send + 'static,
    {
        let mut state = RetryFlowState::new();

        loop {
            let attempt_timeout =
                self.effective_attempt_timeout(state.operation_elapsed, state.total_elapsed());
            self.ensure_elapsed_budget_available(&mut state, attempt_timeout)?;

            let attempt_timeout =
                self.effective_attempt_timeout(state.operation_elapsed, state.total_elapsed());
            self.emit_before_attempt_for_next_attempt(&mut state, attempt_timeout);
            let attempt_timeout =
                self.effective_attempt_timeout(state.operation_elapsed, state.total_elapsed());
            self.ensure_elapsed_budget_available(&mut state, attempt_timeout)?;

            let attempt_start = Instant::now();
            let outcome = call_blocking_attempt(
                Arc::clone(&operation),
                attempt_timeout.duration,
                self.options().worker_cancel_grace(),
            );
            let attempt_elapsed = attempt_start.elapsed();
            state.add_operation_elapsed(attempt_elapsed);
            let context = self
                .context_from_state(&state, attempt_elapsed, attempt_timeout.duration)
                .with_attempt_timeout_source(attempt_timeout.source)
                .with_unreaped_worker_count(outcome.unreaped_worker_count);
            match outcome.result {
                Ok(()) => {
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
                        return Err(self.emit_error(RetryError::new(
                            reason,
                            Some(failure),
                            context,
                        )));
                    }
                    let retry_block_reason = (context.unreaped_worker_count() > 0)
                        .then_some(RetryErrorReason::WorkerStillRunning);
                    match self.handle_failure(
                        state.attempts,
                        failure,
                        context,
                        retry_block_reason,
                        state.started_at,
                    ) {
                        RetryFlowAction::Retry { delay, failure } => {
                            sleep_blocking(delay);
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                    }
                }
            }
        }
    }

    /// Runs a blocking operation with retry and per-attempt timeout isolation.
    ///
    /// This method is a compatibility alias for [`Retry::run_in_worker`]. It
    /// also runs attempts in worker threads when no timeout is configured, so
    /// worker panics are reported as [`crate::AttemptFailure::Panic`] instead of
    /// unwinding through the caller. Worker-spawn failures are reported as
    /// [`crate::AttemptFailure::Executor`].
    ///
    /// # Parameters
    /// - `operation`: Thread-safe operation called once per attempt. It receives
    ///   a cooperative cancellation token for that attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Does not propagate operation panics. Listener panic behavior follows this
    /// retry policy's listener isolation setting.
    ///
    /// # Blocking
    /// Blocks the current thread while waiting for each worker result or timeout
    /// and while sleeping between retry attempts.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time. Worker attempts use
    /// the shortest of configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget as
    /// their effective timeout.
    #[inline]
    pub fn run_blocking_with_timeout<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        self.run_in_worker(operation)
    }
}

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
fn call_blocking_attempt<E>(
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

/// Converts a blocking worker receive result into an attempt outcome.
///
/// # Parameters
/// - `result`: Result from waiting for the worker without an attempt timeout.
/// - `worker`: Worker thread handle for joining the finished worker.
///
/// # Returns
/// The attempt outcome. A disconnected worker is reported as an executor
/// failure instead of panicking the caller.
fn worker_recv_result_to_attempt_outcome<E>(
    result: Result<Result<(), AttemptFailure<E>>, mpsc::RecvError>,
    worker: JoinHandle<()>,
) -> BlockingAttemptOutcome<(), E> {
    join_finished_worker(worker);
    match result {
        Ok(result) => BlockingAttemptOutcome::new(result, 0),
        Err(_) => BlockingAttemptOutcome::new(worker_disconnected_failure!(), 0),
    }
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
    match result {
        Ok(result) => {
            join_finished_worker(worker);
            BlockingAttemptOutcome::new(result, 0)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            token.cancel();
            let worker_exited = wait_for_cancelled_worker(&receiver, worker, worker_cancel_grace);
            let unreaped_worker_count = if worker_exited { 0 } else { 1 };
            BlockingAttemptOutcome::new(Err(AttemptFailure::Timeout), unreaped_worker_count)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            join_finished_worker(worker);
            BlockingAttemptOutcome::new(worker_disconnected_failure!(), 0)
        }
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
        match receiver.try_recv() {
            Ok(_) | Err(mpsc::TryRecvError::Disconnected) => true,
            Err(mpsc::TryRecvError::Empty) => false,
        }
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
