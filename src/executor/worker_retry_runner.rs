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
use std::time::{
    Duration,
    Instant,
};

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_value_operation::BlockingValueOperation;
use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use super::sync_retry_runner::sleep_blocking;
use super::worker_attempt_executor::WorkerAttemptExecutor;
use crate::{
    RetryError,
    RetryErrorReason,
};

/// Runs retry flows using one worker thread per attempt.
pub(in crate::executor) struct WorkerRetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
}

#[allow(clippy::result_large_err)]
impl<'a, E> WorkerRetryRunner<'a, E> {
    /// Creates a worker-thread retry runner.
    ///
    /// # Parameters
    /// - `retry`: Retry policy facade.
    ///
    /// # Returns
    /// A runner borrowing the retry policy.
    #[inline]
    pub(in crate::executor) fn new(retry: &'a Retry<E>) -> Self {
        Self { retry }
    }

    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// # Parameters
    /// - `operation`: Thread-safe operation called once per attempt. It receives
    ///   a cooperative cancellation token for that attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    pub(in crate::executor) fn run<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let operation = Arc::new(BlockingValueOperation::new(operation));
        let worker_operation: Arc<dyn BlockingAttempt<E>> = operation.clone();
        self.run_operation(worker_operation)
            .map(|()| operation.take_value())
    }

    /// Runs a type-erased blocking operation with retry inside worker-thread attempts.
    ///
    /// # Parameters
    /// - `operation`: Shared type-erased operation called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    fn run_operation(&self, operation: Arc<dyn BlockingAttempt<E>>) -> Result<(), RetryError<E>>
    where
        E: Send + 'static,
    {
        let options = self.retry.options();
        let events = self.retry.events();
        let handler = RetryFailureHandler::new(options, events);
        let mut state = RetryFlowState::new();

        loop {
            let attempt_timeout =
                options.effective_attempt_timeout(state.operation_elapsed(), state.total_elapsed());
            if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
                return Err(events.error(error));
            }

            let attempt_timeout =
                options.effective_attempt_timeout(state.operation_elapsed(), state.total_elapsed());
            state.start_next_attempt();
            let context = state.context(options, Duration::ZERO, attempt_timeout);
            events.before_attempt(&context);
            let attempt_timeout =
                options.effective_attempt_timeout(state.operation_elapsed(), state.total_elapsed());
            if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
                return Err(events.error(error));
            }

            let attempt_start = Instant::now();
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&operation),
                attempt_timeout.duration(),
                options.worker_cancel_grace(),
            );
            let attempt_elapsed = attempt_start.elapsed();
            state.add_operation_elapsed(attempt_elapsed);
            let context = state
                .context(options, attempt_elapsed, attempt_timeout)
                .with_unreaped_worker_count(outcome.unreaped_worker_count);
            match outcome.result {
                Ok(()) => {
                    events.attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
                        return Err(events.error(RetryError::new(reason, Some(failure), context)));
                    }
                    let retry_block_reason = (context.unreaped_worker_count() > 0)
                        .then_some(RetryErrorReason::WorkerStillRunning);
                    match handler.handle(&state, failure, context, retry_block_reason) {
                        RetryFlowAction::Retry { delay, failure } => {
                            sleep_blocking(delay);
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(events.error(error)),
                    }
                }
            }
        }
    }
}
