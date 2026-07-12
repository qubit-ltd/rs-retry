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
//! enforce per-attempt timeouts by racing the future against the policy's
//! injected [`qubit_clock::AsyncSleeper`], but it does not create a panic
//! boundary; operation panics still unwind the async task.

use std::future::Future;
use std::time::Duration;

use qubit_clock::{
    AsyncSleeper,
    TimeError,
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
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying
    /// stops.
    async fn run_operation(
        &self,
        operation: &mut dyn AsyncAttempt<E>,
    ) -> Result<(), RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        let sleeper = self.retry.async_sleeper();
        let handler = RetryFailureHandler::new(options, events);
        let mut state = RetryFlowState::new(sleeper);

        loop {
            // The effective timeout may be selected by the configured
            // per-attempt timeout or by whichever elapsed budget has the least
            // remaining time. It is recomputed at every control point because
            // listeners run on the retry path and can consume total elapsed
            // budget before the user future is created.
            let attempt_timeout = options.effective_attempt_timeout(
                state.operation_elapsed(),
                state.total_elapsed(),
            );
            if let Some(error) =
                state.take_elapsed_error(options, attempt_timeout)
            {
                return Err(events.error(error));
            }

            let attempt_timeout = options.effective_attempt_timeout(
                state.operation_elapsed(),
                state.total_elapsed(),
            );
            state.start_next_attempt();
            let context =
                state.context(options, Duration::ZERO, attempt_timeout);
            events.before_attempt(&context);
            let attempt_timeout = options.effective_attempt_timeout(
                state.operation_elapsed(),
                state.total_elapsed(),
            );
            if let Some(error) =
                state.take_elapsed_error(options, attempt_timeout)
            {
                return Err(events.error(error));
            }

            // Async timeout is enforced by dropping the future after the Tokio
            // timer fires. The timeout source is kept in the context so a later
            // timeout failure can be classified as configured timeout vs an
            // elapsed-budget terminal stop.
            let attempt_start = sleeper.now();
            let result = if let Some(timeout) = attempt_timeout.duration() {
                tokio::select! {
                    biased;
                    timeout_result = sleeper.sleep_for_async(timeout) => {
                        match timeout_result {
                            Ok(()) => Err(AttemptFailure::Timeout),
                            Err(error) => {
                                return Err(events.error(
                                    state.sleeper_error(options, error),
                                ));
                            }
                        }
                    }
                    result = operation.call() => result,
                }
            } else {
                operation.call().await
            };

            let attempt_elapsed = state.elapsed_since(attempt_start);
            state.add_operation_elapsed(attempt_elapsed);
            let context =
                state.context(options, attempt_elapsed, attempt_timeout);
            match result {
                Ok(()) => {
                    events.attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    if let Some(reason) =
                        attempt_timeout.elapsed_timeout_reason(&failure)
                    {
                        // A timeout caused by an elapsed budget is already
                        // terminal. Only configured attempt timeouts are routed
                        // through the normal failure policy.
                        return Err(events.error(RetryError::new(
                            reason,
                            Some(failure),
                            context,
                        )));
                    }
                    match handler.handle(&state, failure, context, None) {
                        RetryFlowAction::Retry { delay, failure } => {
                            if let Err(error) =
                                sleep_async(sleeper, delay).await
                            {
                                return Err(events.error(
                                    state.sleeper_error(options, error),
                                ));
                            }
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => {
                            return Err(events.error(error));
                        }
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
async fn sleep_async(
    sleeper: &dyn AsyncSleeper,
    delay: Duration,
) -> Result<(), TimeError> {
    if !delay.is_zero() {
        sleeper.sleep_for_async(delay).await?;
    }
    Ok(())
}
