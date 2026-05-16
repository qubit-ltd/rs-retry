/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Synchronous and asynchronous retry runners.

#[cfg(feature = "tokio")]
use std::future::Future;
use std::time::{
    Duration,
    Instant,
};

#[cfg(feature = "tokio")]
use super::async_attempt::AsyncAttempt;
#[cfg(feature = "tokio")]
use super::async_value_operation::AsyncValueOperation;
use super::effective_attempt_timeout::EffectiveAttemptTimeout;
use super::retry::{
    Retry,
    sleep_blocking,
};
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use super::sync_attempt::SyncAttempt;
use super::sync_value_operation::SyncValueOperation;
#[cfg(feature = "tokio")]
use crate::AttemptFailure;
use crate::event::RetryContextParts;
use crate::{
    AttemptTimeoutSource,
    RetryContext,
    RetryError,
    RetryErrorReason,
};

/// Synchronous retry execution for retry policies.
#[allow(clippy::result_large_err)]
impl<E> Retry<E> {
    /// Runs a synchronous operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Operation called once per attempt until it succeeds or the
    ///   retry flow stops.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Propagates operation panics and listener panics unless listener panic
    /// isolation is enabled.
    ///
    /// # Blocking
    /// Blocks the current thread with `std::thread::sleep` between attempts when
    /// a non-zero retry delay is selected.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time, including
    /// operation execution, retry sleep, retry-after sleep, and retry
    /// control-path listener time. This synchronous mode cannot interrupt an
    /// already-running operation; it checks budgets before attempts and after
    /// failed attempts. If `attempt_timeout` is configured, this method returns
    /// [`RetryErrorReason::UnsupportedOperation`] because timeout enforcement
    /// requires worker-thread or async execution.
    pub fn run<T, F>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        if self.options().attempt_timeout().is_some() {
            let attempt_timeout = self.attempt_timeout_duration();
            return Err(self.emit_error(RetryError::new(
                RetryErrorReason::UnsupportedOperation,
                None,
                RetryContext::from_parts(RetryContextParts {
                    attempt: 0,
                    max_attempts: self.options().max_attempts(),
                    max_operation_elapsed: self.options().max_operation_elapsed(),
                    max_total_elapsed: self.options().max_total_elapsed(),
                    operation_elapsed: Duration::ZERO,
                    total_elapsed: Duration::ZERO,
                    attempt_elapsed: Duration::ZERO,
                    attempt_timeout,
                })
                .with_attempt_timeout_source(Some(AttemptTimeoutSource::Configured)),
            )));
        }
        let mut operation = SyncValueOperation::new(&mut operation);
        match self.run_sync_operation(&mut operation) {
            Ok(()) => Ok(operation.into_value()),
            Err(error) => Err(error),
        }
    }

    /// Runs a synchronous value-erased operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    fn run_sync_operation(&self, operation: &mut dyn SyncAttempt<E>) -> Result<(), RetryError<E>> {
        let mut state = RetryFlowState::new();

        loop {
            self.ensure_elapsed_budget_available(
                &mut state,
                EffectiveAttemptTimeout::new(None, None),
            )?;

            self.emit_before_attempt_for_next_attempt(
                &mut state,
                EffectiveAttemptTimeout::new(None, None),
            );
            self.ensure_elapsed_budget_available(
                &mut state,
                EffectiveAttemptTimeout::new(None, None),
            )?;

            let attempt_start = Instant::now();
            match operation.call() {
                Ok(()) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    state.add_operation_elapsed(attempt_elapsed);
                    let context = self.context_from_state(&state, attempt_elapsed, None);
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    state.add_operation_elapsed(attempt_elapsed);
                    let context = self.context_from_state(&state, attempt_elapsed, None);
                    match self.handle_failure(
                        state.attempts,
                        failure,
                        context,
                        None,
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
}

/// Asynchronous retry execution for retry policies.
#[cfg(feature = "tokio")]
#[allow(clippy::result_large_err)]
impl<E> Retry<E> {
    /// Runs an asynchronous operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Factory returning a fresh future for each attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Propagates operation panics from the current async task. They are not
    /// converted to [`AttemptFailure::Panic`] because `run_async` does not
    /// create an isolation boundary. Listener panics are propagated unless
    /// listener panic isolation is enabled. Tokio may panic if timer APIs are
    /// used outside a runtime with a time driver.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time. Async attempts use
    /// the shortest of configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget as
    /// their effective timeout.
    pub async fn run_async<T, F, Fut>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut operation = AsyncValueOperation::new(&mut operation);
        self.run_async_operation(&mut operation)
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
    async fn run_async_operation(
        &self,
        operation: &mut dyn AsyncAttempt<E>,
    ) -> Result<(), RetryError<E>> {
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
            let result = if let Some(timeout) = attempt_timeout.duration {
                match tokio::time::timeout(timeout, operation.call()).await {
                    Ok(result) => result,
                    Err(_) => Err(AttemptFailure::Timeout),
                }
            } else {
                operation.call().await
            };

            let attempt_elapsed = attempt_start.elapsed();
            state.add_operation_elapsed(attempt_elapsed);
            let context = self
                .context_from_state(&state, attempt_elapsed, attempt_timeout.duration)
                .with_attempt_timeout_source(attempt_timeout.source);
            match result {
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
                    match self.handle_failure(
                        state.attempts,
                        failure,
                        context,
                        None,
                        state.started_at,
                    ) {
                        RetryFlowAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            state.record_last_failure(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
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
#[cfg(feature = "tokio")]
async fn sleep_async(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}
