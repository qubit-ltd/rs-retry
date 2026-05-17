/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Asynchronous retry runner.

use std::future::Future;
use std::time::{
    Duration,
    Instant,
};

use super::async_attempt::AsyncAttempt;
use super::async_value_operation::AsyncValueOperation;
use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::{
    AttemptFailure,
    RetryError,
};

/// Runs retry flows on the current asynchronous task.
pub(in crate::executor) struct AsyncRetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
}

#[allow(clippy::result_large_err)]
impl<'a, E> AsyncRetryRunner<'a, E> {
    /// Creates an asynchronous retry runner.
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

    /// Runs an asynchronous operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Factory returning a fresh future for each attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    pub(in crate::executor) async fn run<T, F, Fut>(
        &self,
        mut operation: F,
    ) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut operation = AsyncValueOperation::new(&mut operation);
        self.run_operation(&mut operation)
            .await
            .map(|()| operation.into_value())
    }

    /// Runs an asynchronous value-erased operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Async operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    async fn run_operation(
        &self,
        operation: &mut dyn AsyncAttempt<E>,
    ) -> Result<(), RetryError<E>> {
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
            let result = if let Some(timeout) = attempt_timeout.duration() {
                match tokio::time::timeout(timeout, operation.call()).await {
                    Ok(result) => result,
                    Err(_) => Err(AttemptFailure::Timeout),
                }
            } else {
                operation.call().await
            };

            let attempt_elapsed = attempt_start.elapsed();
            state.add_operation_elapsed(attempt_elapsed);
            let context = state.context(options, attempt_elapsed, attempt_timeout);
            match result {
                Ok(()) => {
                    events.attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
                        return Err(events.error(RetryError::new(reason, Some(failure), context)));
                    }
                    match handler.handle(&state, failure, context, None) {
                        RetryFlowAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(events.error(error)),
                    }
                }
            }
        }
    }
}

/// Sleeps asynchronously when the delay is non-zero.
///
/// # Parameters
/// - `delay`: Delay to sleep.
///
/// # Returns
/// This function returns after the sleep completes.
async fn sleep_async(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}
