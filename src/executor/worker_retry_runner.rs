// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Worker-thread retry runner.
//!
//! This runner gives each attempt its own thread boundary. That boundary lets
//! the retry flow capture panics, wait with a timeout, and request cooperative
//! cancellation through [`AttemptCancelToken`]. It still cannot kill Rust
//! threads; an attempt that ignores cancellation may remain detached, which is
//! reported as `WorkerStillRunning` before another worker can be spawned.

use std::sync::Arc;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_value_operation::BlockingValueOperation;
use super::internal::{complete_attempt, prepare_timed_attempt};
use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use super::retry_runner::sleep_blocking;
use super::worker_attempt_executor::WorkerAttemptExecutor;
use crate::options::EffectiveAttemptTimeout;
use crate::{AttemptFailure, RetryContext, RetryError, RetryErrorReason};

/// Runs retry flows using one worker thread per attempt.
pub(in crate::executor) struct WorkerRetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
}

#[allow(clippy::result_large_err)]
impl<'a, E> WorkerRetryRunner<'a, E> {
    /// Creates a worker-thread retry runner.
    ///
    /// # Arguments
    /// - `retry`: Retry policy facade.
    ///
    /// # Returns
    /// A runner borrowing the retry policy.
    #[inline(always)]
    pub(in crate::executor) fn new(retry: &'a Retry<E>) -> Self {
        Self { retry }
    }

    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// # Arguments
    /// - `operation`: Thread-safe operation called once per attempt. It
    ///   receives a cooperative cancellation token for that attempt.
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

    /// Runs a type-erased blocking operation with retry inside worker-thread
    /// attempts.
    ///
    /// # Arguments
    /// - `operation`: Shared type-erased operation called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying
    /// stops.
    fn run_operation(&self, operation: Arc<dyn BlockingAttempt<E>>) -> Result<(), RetryError<E>>
    where
        E: Send + 'static,
    {
        let options = self.retry.options();
        let events = self.retry.events();
        let sleeper = self.retry.blocking_sleeper();
        let handler = RetryFailureHandler::new(options, events);
        let mut state = RetryFlowState::new(sleeper.timer().clock());

        loop {
            let attempt_timeout = prepare_timed_attempt(&mut state, options, events)
                .map_err(|error| events.error(error))?;

            // WorkerAttemptExecutor owns the thread-level details for a single
            // attempt. The runner only turns the resulting attempt outcome into
            // retry-flow state and policy decisions.
            let attempt_start = sleeper.timer().clock().now();
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&operation),
                attempt_timeout.duration(),
                options.worker_cancel_grace(),
            );
            let context = complete_attempt(&mut state, options, attempt_start, attempt_timeout)
                .with_unreaped_worker_count(outcome.unreaped_worker_count);
            match outcome.result {
                Ok(()) => {
                    events.attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    self.handle_failure(&mut state, &handler, attempt_timeout, failure, context)?;
                }
            }
        }
    }

    /// Handles a worker attempt failure and any blocking retry sleep.
    ///
    /// # Arguments
    ///
    /// * `state` - Mutable state for the active retry flow.
    /// * `handler` - Shared failure policy and observation pipeline.
    /// * `attempt_timeout` - Effective timeout used by the failed attempt.
    /// * `failure` - Failure produced by the committed attempt.
    /// * `context` - Completed-attempt context, including worker reap state.
    ///
    /// # Errors
    ///
    /// Returns a hard elapsed-timeout error, a worker safety error, another
    /// terminal retry error, or a sleeper error.
    fn handle_failure(
        &self,
        state: &mut RetryFlowState<'_, E>,
        handler: &RetryFailureHandler<'_, E>,
        attempt_timeout: EffectiveAttemptTimeout,
        failure: AttemptFailure<E>,
        context: RetryContext,
    ) -> Result<(), RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        let sleeper = self.retry.blocking_sleeper();
        if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
            let error = handler.elapsed_timeout_error(state, failure, context, reason);
            return Err(events.error(error));
        }
        // Starting another worker while a timed-out one is still running would
        // allow concurrent attempts for one flow, so it is a hard safety stop.
        let retry_block_reason =
            (context.unreaped_worker_count() > 0).then_some(RetryErrorReason::WorkerStillRunning);
        match handler.handle(state, failure, context, retry_block_reason) {
            RetryFlowAction::Retry { delay, failure } => {
                sleep_blocking(sleeper, delay)
                    .map_err(|error| events.error(state.sleeper_error(options, error)))?;
                state.record_last_failure(failure);
                Ok(())
            }
            RetryFlowAction::Finished(error) => Err(events.error(error)),
        }
    }
}
