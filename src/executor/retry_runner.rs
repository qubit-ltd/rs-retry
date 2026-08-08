// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Same-thread synchronous retry runner.
//!
//! This runner is the simplest execution mode: the caller's closure is invoked
//! directly on the current thread and retry sleeps use the policy's injected
//! [`qubit_clock::BlockingSleeper`].
//! Because there is no cancellation boundary, configured per-attempt timeout is
//! rejected before the first attempt instead of being simulated unsafely.

use std::time::Duration;

use qubit_clock::{BlockingSleeper, TimeError};

use super::attempt::Attempt;
use super::internal::{complete_attempt, prepare_same_thread_attempt};
use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use super::value_operation::ValueOperation;
use crate::options::EffectiveAttemptTimeout;
use crate::{
    AttemptFailure, AttemptTimeoutSource, RetryContext, RetryError, RetryErrorReason, RetryResult,
    RetrySuccess,
};

/// Runs retry flows on the current thread.
pub(in crate::executor) struct RetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
}

#[allow(clippy::result_large_err)]
impl<'a, E> RetryRunner<'a, E> {
    /// Creates a synchronous retry runner.
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

    /// Runs a synchronous operation with retry.
    ///
    /// # Arguments
    /// - `operation`: Operation called once per attempt until it succeeds or
    ///   the retry flow stops.
    ///
    /// # Returns
    /// `Ok(RetrySuccess<T>)` with the operation value and final retry context,
    /// or [`RetryError`] when retrying stops.
    pub(in crate::executor) fn run<T, F>(&self, mut operation: F) -> RetryResult<T, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        if self.retry.options().attempt_timeout().is_some() {
            return Err(self.unsupported_attempt_timeout_error());
        }
        let mut operation = ValueOperation::new(&mut operation);
        self.run_operation(&mut operation)
            .map(|context| RetrySuccess::new(operation.into_value(), context))
    }

    /// Runs a synchronous value-erased operation with retry.
    ///
    /// # Arguments
    /// - `operation`: Operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying
    /// stops.
    fn run_operation(&self, operation: &mut dyn Attempt<E>) -> Result<RetryContext, RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        let sleeper = self.retry.blocking_sleeper();
        let handler = RetryFailureHandler::new(options, events, self.retry.random_source());
        let mut state = RetryFlowState::new(sleeper.timer().clock());

        loop {
            let attempt_timeout = prepare_same_thread_attempt(&mut state, options, events)
                .map_err(|error| events.error(error))?;

            // Only user closure time contributes to max_operation_elapsed.
            // Listener time and retry sleeps are included by total_elapsed
            // through RetryFlowState's monotonic start instant.
            let attempt_start = sleeper.timer().clock().now();
            let result = operation.call();
            let context = complete_attempt(&mut state, options, attempt_start, attempt_timeout);
            match result {
                Ok(()) => {
                    events.attempt_success(&context);
                    return Ok(context);
                }
                Err(failure) => {
                    self.handle_failure(&mut state, &handler, failure, context)?;
                }
            }
        }
    }

    /// Handles a same-thread attempt failure and any blocking retry sleep.
    ///
    /// # Arguments
    ///
    /// * `state` - Mutable state for the active retry flow.
    /// * `handler` - Shared failure policy and observation pipeline.
    /// * `failure` - Failure produced by the committed attempt.
    /// * `context` - Completed-attempt context.
    ///
    /// # Errors
    ///
    /// Returns a terminal retry error or a sleeper error when another attempt
    /// cannot be scheduled.
    fn handle_failure(
        &self,
        state: &mut RetryFlowState<'_, E>,
        handler: &RetryFailureHandler<'_, E>,
        failure: AttemptFailure<E>,
        context: RetryContext,
    ) -> Result<(), RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        let sleeper = self.retry.blocking_sleeper();
        match handler.handle(state, failure, context, None) {
            RetryFlowAction::Retry { delay, failure } => {
                // Retain the failure only after the retry sleep succeeds. It
                // then remains available if the next pre-attempt check stops.
                sleep_blocking(sleeper, delay)
                    .map_err(|error| events.error(state.sleeper_error(options, error)))?;
                state.record_last_failure(failure);
                Ok(())
            }
            RetryFlowAction::Finished(error) => Err(events.error(error)),
        }
    }

    /// Builds an unsupported-operation error for configured attempt timeout.
    ///
    /// # Returns
    /// A retry error explaining that same-thread sync execution cannot enforce
    /// per-attempt timeout.
    fn unsupported_attempt_timeout_error(&self) -> RetryError<E> {
        let options = self.retry.options();
        let state: RetryFlowState<'_, E> =
            RetryFlowState::new(self.retry.blocking_sleeper().timer().clock());
        let attempt_timeout = EffectiveAttemptTimeout::new(
            options.attempt_timeout_duration(),
            Some(AttemptTimeoutSource::Configured),
        );
        self.retry.events().error(RetryError::new(
            RetryErrorReason::UnsupportedOperation,
            None,
            state.context(options, Duration::ZERO, attempt_timeout),
        ))
    }
}

/// Sleeps the current thread when the delay is non-zero.
///
/// # Arguments
/// - `delay`: Delay to sleep.
pub(in crate::executor) fn sleep_blocking(
    sleeper: &BlockingSleeper,
    delay: Duration,
) -> Result<(), TimeError> {
    if !delay.is_zero() {
        sleeper.sleep_for(delay)?;
    }
    Ok(())
}
