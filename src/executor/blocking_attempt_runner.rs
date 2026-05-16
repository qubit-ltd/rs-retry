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

use std::sync::Arc;
use std::time::Instant;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_value_operation::BlockingValueOperation;
use super::retry::{
    Retry,
    call_blocking_attempt,
    sleep_blocking,
};
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::{
    RetryError,
    RetryErrorReason,
};

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
