// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Asynchronous retry runner.
//!
//! This runner executes each attempt future on the current Tokio task. It can
//! enforce per-attempt timeouts by racing the future against the selected
//! [`qubit_clock::Timer`], but it does not create a panic boundary;
//! operation panics still unwind the async task.

use std::future::Future;
use std::time::Duration;

use qubit_clock::{TimeError, Timer};

use super::internal::{complete_attempt, prepare_timed_attempt};
use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::options::EffectiveAttemptTimeout;
use crate::{AttemptFailure, RetryContext, RetryError, RetryResult, RetrySuccess};

/// Runs retry flows on the current asynchronous task.
pub(in crate::executor) struct AsyncRetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
    /// Timer and monotonic clock bound to this async execution.
    timer: &'a dyn Timer,
}

#[allow(clippy::result_large_err)]
impl<'a, E> AsyncRetryRunner<'a, E> {
    /// Creates an asynchronous retry runner.
    ///
    /// # Arguments
    /// - `retry`: Retry policy facade.
    /// - `timer`: Timer used for elapsed time, timeouts, and backoff.
    ///
    /// # Returns
    /// A runner borrowing the retry policy.
    #[inline(always)]
    pub(in crate::executor) fn new(retry: &'a Retry<E>, timer: &'a dyn Timer) -> Self {
        Self { retry, timer }
    }

    /// Runs an asynchronous operation with retry.
    ///
    /// # Arguments
    /// - `operation`: Factory returning a fresh future for each attempt.
    ///
    /// # Returns
    /// `Ok(RetrySuccess<T>)` with the operation value and final retry context,
    /// or [`RetryError`] when retrying stops.
    pub(in crate::executor) async fn run<T, F, Fut>(&self, mut operation: F) -> RetryResult<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let options = self.retry.options();
        let events = self.retry.events();
        let timer = self.timer;
        let handler = RetryFailureHandler::new(options, events, self.retry.random_source());
        let mut state = RetryFlowState::new(timer.clock());

        loop {
            let attempt_timeout = prepare_timed_attempt(&mut state, options, events)
                .map_err(|error| events.error(error))?;

            // Async timeout is enforced by dropping the future after the Tokio
            // timer fires. The timeout source is kept in the context so a later
            // timeout failure can be classified as configured timeout vs an
            // elapsed-budget terminal stop.
            let attempt_start = timer.clock().now();
            let result = if let Some(timeout) = attempt_timeout.duration() {
                let timeout_future = timer
                    .after(timeout)
                    .map_err(|error| events.error(state.sleeper_error(options, error)))?;
                tokio::select! {
                    biased;
                    result = timeout_future => match result {
                        Ok(()) => Err(AttemptFailure::Timeout),
                        Err(error) => {
                            return Err(events.error(
                                state.sleeper_error(options, error),
                            ));
                        }
                    },
                    result = operation() => result.map_err(AttemptFailure::Error),
                }
            } else {
                operation().await.map_err(AttemptFailure::Error)
            };

            let context = complete_attempt(&mut state, options, attempt_start, attempt_timeout);
            match result {
                Ok(value) => {
                    events.attempt_success(&context);
                    return Ok(RetrySuccess::new(value, context));
                }
                Err(failure) => {
                    self.handle_failure(&mut state, &handler, attempt_timeout, failure, context)
                        .await?;
                }
            }
        }
    }

    /// Handles an async attempt failure and any asynchronous retry sleep.
    ///
    /// # Arguments
    ///
    /// * `state` - Mutable state for the active retry flow.
    /// * `handler` - Shared failure policy and observation pipeline.
    /// * `attempt_timeout` - Effective timeout used by the failed attempt.
    /// * `failure` - Failure produced by the committed attempt.
    /// * `context` - Completed-attempt context.
    ///
    /// # Errors
    ///
    /// Returns a hard elapsed-timeout error, another terminal retry error, or a
    /// sleeper error when another attempt cannot be scheduled.
    async fn handle_failure(
        &self,
        state: &mut RetryFlowState<'_, E>,
        handler: &RetryFailureHandler<'_, E>,
        attempt_timeout: EffectiveAttemptTimeout,
        failure: AttemptFailure<E>,
        context: RetryContext,
    ) -> Result<(), RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
            let error = handler.elapsed_timeout_error(state, failure, context, reason);
            return Err(events.error(error));
        }
        match handler.handle(state, failure, context, None) {
            RetryFlowAction::Retry { delay, failure } => {
                sleep_async(self.timer, delay)
                    .await
                    .map_err(|error| events.error(state.sleeper_error(options, error)))?;
                state.record_last_failure(failure);
                Ok(())
            }
            RetryFlowAction::Finished(error) => Err(events.error(error)),
        }
    }
}

/// Sleeps asynchronously when the delay is non-zero.
///
/// # Arguments
/// - `delay`: Delay to sleep.
///
/// # Returns
/// This function returns after the sleep completes.
async fn sleep_async(timer: &dyn Timer, delay: Duration) -> Result<(), TimeError> {
    if !delay.is_zero() {
        timer.after(delay)?.await?;
    }
    Ok(())
}
