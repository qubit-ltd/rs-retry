/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Same-thread synchronous retry runner.
//!
//! This runner is the simplest execution mode: the caller's closure is invoked
//! directly on the current thread and retry sleeps use `std::thread::sleep`.
//! Because there is no cancellation boundary, configured per-attempt timeout is
//! rejected before the first attempt instead of being simulated unsafely.

use std::time::{
    Duration,
    Instant,
};

use super::retry::Retry;
use super::retry_failure_handler::RetryFailureHandler;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use super::sync_attempt::SyncAttempt;
use super::sync_value_operation::SyncValueOperation;
use crate::options::EffectiveAttemptTimeout;
use crate::{
    AttemptTimeoutSource,
    RetryError,
    RetryErrorReason,
};

/// Runs retry flows on the current thread.
pub(in crate::executor) struct SyncRetryRunner<'a, E> {
    /// Retry policy facade that owns options and events.
    retry: &'a Retry<E>,
}

#[allow(clippy::result_large_err)]
impl<'a, E> SyncRetryRunner<'a, E> {
    /// Creates a synchronous retry runner.
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

    /// Runs a synchronous operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Operation called once per attempt until it succeeds or the
    ///   retry flow stops.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    pub(in crate::executor) fn run<T, F>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        if self.retry.options().attempt_timeout().is_some() {
            return Err(self.unsupported_attempt_timeout_error());
        }
        let mut operation = SyncValueOperation::new(&mut operation);
        self.run_operation(&mut operation)
            .map(|()| operation.into_value())
    }

    /// Runs a synchronous value-erased operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    fn run_operation(&self, operation: &mut dyn SyncAttempt<E>) -> Result<(), RetryError<E>> {
        let options = self.retry.options();
        let events = self.retry.events();
        let handler = RetryFailureHandler::new(options, events);
        let no_timeout = EffectiveAttemptTimeout::none();
        let mut state = RetryFlowState::new();

        loop {
            // Same-thread execution cannot interrupt a running closure. Budget
            // checks therefore happen only at points where control has returned
            // to the retry loop: before preparing an attempt and after listener
            // callbacks that may have consumed total elapsed time.
            if let Some(error) = state.take_elapsed_error(options, no_timeout) {
                return Err(events.error(error));
            }

            state.start_next_attempt();
            let context = state.context(options, Duration::ZERO, no_timeout);
            events.before_attempt(&context);
            if let Some(error) = state.take_elapsed_error(options, no_timeout) {
                return Err(events.error(error));
            }

            // Only user closure time contributes to max_operation_elapsed.
            // Listener time and retry sleeps are included by total_elapsed
            // through RetryFlowState's monotonic start instant.
            let attempt_start = Instant::now();
            match operation.call() {
                Ok(()) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    state.add_operation_elapsed(attempt_elapsed);
                    let context = state.context(options, attempt_elapsed, no_timeout);
                    events.attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    state.add_operation_elapsed(attempt_elapsed);
                    let context = state.context(options, attempt_elapsed, no_timeout);
                    match handler.handle(&state, failure, context, None) {
                        RetryFlowAction::Retry { delay, failure } => {
                            // Keep the failure only after the policy has
                            // committed to another attempt. If the sleep or the
                            // next pre-attempt check exhausts a budget, this is
                            // the last meaningful failure to attach to the
                            // terminal RetryError.
                            sleep_blocking(delay);
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(events.error(error)),
                    }
                }
            }
        }
    }

    /// Builds an unsupported-operation error for configured attempt timeout.
    ///
    /// # Returns
    /// A retry error explaining that same-thread sync execution cannot enforce
    /// per-attempt timeout.
    fn unsupported_attempt_timeout_error(&self) -> RetryError<E> {
        let options = self.retry.options();
        let state: RetryFlowState<E> = RetryFlowState::new();
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
/// # Parameters
/// - `delay`: Delay to sleep.
pub(in crate::executor) fn sleep_blocking(delay: Duration) {
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}
