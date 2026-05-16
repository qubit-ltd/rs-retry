/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry execution.
//!
//! A [`Retry`] owns validated retry options and lifecycle listeners. The
//! operation success type is introduced by each `run` call, while the error type
//! is bound by the retry policy.

use qubit_error::BoxError;
use qubit_function::{
    BiConsumer,
    BiFunction,
    Consumer,
};
use std::fmt;
use std::time::{
    Duration,
    Instant,
};

use super::effective_attempt_timeout::EffectiveAttemptTimeout;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::event::{
    RetryContextParts,
    RetryListeners,
};
use crate::{
    AttemptFailure,
    AttemptFailureDecision,
    AttemptTimeoutPolicy,
    AttemptTimeoutSource,
    RetryAfterHint,
    RetryBuilder,
    RetryConfigError,
    RetryContext,
    RetryError,
    RetryErrorReason,
    RetryOptions,
};

/// Retry policy and executor bound to an operation error type.
///
/// The generic parameter `E` is the caller's operation error type. Cloning a
/// retry policy shares all registered functors through reference-counted
/// `rs-function` wrappers.
#[derive(Clone)]
pub struct Retry<E = BoxError> {
    /// Validated retry limits and backoff settings.
    options: RetryOptions,
    /// Optional retry-after hint extractor.
    retry_after_hint: Option<RetryAfterHint<E>>,
    /// Whether listener panics should be isolated.
    isolate_listener_panics: bool,
    /// Lifecycle listeners.
    listeners: RetryListeners<E>,
}

/// Retry executor implementation.
#[allow(clippy::result_large_err)]
impl<E> Retry<E> {
    /// Creates a retry builder.
    ///
    /// # Returns
    /// A [`RetryBuilder`] configured with defaults.
    #[inline]
    pub fn builder() -> RetryBuilder<E> {
        RetryBuilder::new()
    }

    /// Creates a retry policy from options.
    ///
    /// # Parameters
    /// - `options`: Retry options to validate and install.
    ///
    /// # Returns
    /// A retry policy using the default listener set.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] if the options are invalid.
    pub fn from_options(options: RetryOptions) -> Result<Self, RetryConfigError> {
        Self::builder().options(options).build()
    }

    /// Returns the immutable options used by this retry policy.
    ///
    /// # Returns
    /// Shared retry options.
    #[inline]
    pub fn options(&self) -> &RetryOptions {
        &self.options
    }

    /// Creates a retry policy from validated parts.
    ///
    /// # Parameters
    /// - `options`: Retry options.
    /// - `retry_after_hint`: Optional hint extractor.
    /// - `isolate_listener_panics`: Whether listener panics are isolated.
    /// - `listeners`: Lifecycle listeners.
    ///
    /// # Returns
    /// A retry policy.
    pub(super) fn new(
        options: RetryOptions,
        retry_after_hint: Option<RetryAfterHint<E>>,
        isolate_listener_panics: bool,
        listeners: RetryListeners<E>,
    ) -> Self {
        Self {
            options,
            retry_after_hint,
            isolate_listener_panics,
            listeners,
        }
    }

    /// Checks elapsed budgets and returns a terminal error when exhausted.
    ///
    /// # Parameters
    /// - `state`: Mutable retry-flow state carrying elapsed counters and last failure.
    /// - `attempt_timeout`: Effective timeout visible in the terminal context.
    ///
    /// # Returns
    /// `Ok(())` when retry execution may continue.
    ///
    /// # Errors
    /// Returns a [`RetryError`] when the max-operation-elapsed or
    /// max-total-elapsed budget is exhausted. The retained last failure is moved
    /// into the error when present.
    pub(in crate::executor) fn ensure_elapsed_budget_available(
        &self,
        state: &mut RetryFlowState<E>,
        attempt_timeout: EffectiveAttemptTimeout,
    ) -> Result<(), RetryError<E>> {
        let total_elapsed = state.total_elapsed();
        let Some(reason) = self.elapsed_error_reason(state.operation_elapsed, total_elapsed) else {
            return Ok(());
        };
        Err(self.emit_error(self.elapsed_error(
            reason,
            state.operation_elapsed,
            total_elapsed,
            state.attempts,
            state.take_last_failure(),
            attempt_timeout,
        )))
    }

    /// Emits before-attempt listeners for the next attempt.
    ///
    /// # Parameters
    /// - `state`: Mutable retry-flow state whose attempt counter will be advanced.
    /// - `attempt_timeout`: Effective timeout selected before listener execution.
    pub(in crate::executor) fn emit_before_attempt_for_next_attempt(
        &self,
        state: &mut RetryFlowState<E>,
        attempt_timeout: EffectiveAttemptTimeout,
    ) {
        state.start_next_attempt();
        let before_context = self
            .context_from_state(state, Duration::ZERO, attempt_timeout.duration)
            .with_attempt_timeout_source(attempt_timeout.source);
        self.emit_before_attempt(&before_context);
    }

    /// Builds a context snapshot from retry-flow state.
    ///
    /// # Parameters
    /// - `state`: Retry-flow state carrying attempt and elapsed counters.
    /// - `attempt_elapsed`: Elapsed time in the current attempt.
    /// - `attempt_timeout`: Effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// A retry context using the latest state values.
    pub(in crate::executor) fn context_from_state(
        &self,
        state: &RetryFlowState<E>,
        attempt_elapsed: Duration,
        attempt_timeout: Option<Duration>,
    ) -> RetryContext {
        self.context(
            state.operation_elapsed,
            state.total_elapsed(),
            state.attempts,
            attempt_elapsed,
            attempt_timeout,
        )
    }

    /// Builds a context snapshot.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic time consumed by this flow.
    /// - `attempt`: Current attempt number.
    /// - `attempt_elapsed`: Elapsed time in the current attempt.
    /// - `attempt_timeout`: Effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// A retry context.
    fn context(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
        attempt: u32,
        attempt_elapsed: Duration,
        attempt_timeout: Option<Duration>,
    ) -> RetryContext {
        RetryContext::from_parts(RetryContextParts {
            attempt,
            max_attempts: self.options.max_attempts.get(),
            max_operation_elapsed: self.options.max_operation_elapsed,
            max_total_elapsed: self.options.max_total_elapsed,
            operation_elapsed,
            total_elapsed,
            attempt_elapsed,
            attempt_timeout,
        })
    }

    /// Returns the configured attempt-timeout duration.
    ///
    /// # Returns
    /// `Some(Duration)` when per-attempt timeout is configured.
    #[inline]
    pub(in crate::executor) fn attempt_timeout_duration(&self) -> Option<Duration> {
        self.options
            .attempt_timeout()
            .map(|attempt_timeout| attempt_timeout.timeout())
    }

    /// Returns the effective timeout used by the next attempt.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed so far.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed so far.
    ///
    /// # Returns
    /// The shortest of the configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget,
    /// including the source that selected it. A configured timeout wins ties so
    /// its timeout policy remains observable.
    pub(in crate::executor) fn effective_attempt_timeout(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> EffectiveAttemptTimeout {
        let candidates = [
            self.attempt_timeout_duration()
                .map(|duration| (duration, AttemptTimeoutSource::Configured)),
            self.remaining_operation_elapsed(operation_elapsed)
                .map(|duration| (duration, AttemptTimeoutSource::MaxOperationElapsed)),
            self.remaining_total_elapsed(total_elapsed)
                .map(|duration| (duration, AttemptTimeoutSource::MaxTotalElapsed)),
        ];
        let selected = candidates
            .into_iter()
            .flatten()
            .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        match selected {
            Some((duration, source)) => EffectiveAttemptTimeout::new(Some(duration), Some(source)),
            None => EffectiveAttemptTimeout::new(None, None),
        }
    }

    /// Returns remaining user operation time before the max-operation-elapsed budget is exhausted.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed so far.
    ///
    /// # Returns
    /// `Some(Duration)` when max elapsed is configured, or `None` when unlimited.
    #[inline]
    fn remaining_operation_elapsed(&self, operation_elapsed: Duration) -> Option<Duration> {
        self.options
            .max_operation_elapsed
            .map(|max_operation_elapsed| max_operation_elapsed.saturating_sub(operation_elapsed))
    }

    /// Returns remaining total retry-flow time before the max-total-elapsed budget is exhausted.
    ///
    /// # Parameters
    /// - `total_elapsed`: Total monotonic retry-flow time consumed so far.
    ///
    /// # Returns
    /// `Some(Duration)` when max total elapsed is configured, or `None` when unlimited.
    #[inline]
    fn remaining_total_elapsed(&self, total_elapsed: Duration) -> Option<Duration> {
        self.options
            .max_total_elapsed
            .map(|max_total_elapsed| max_total_elapsed.saturating_sub(total_elapsed))
    }

    /// Handles one failed attempt.
    ///
    /// # Parameters
    /// - `attempts`: Attempts executed so far.
    /// - `failure`: Attempt failure.
    /// - `context`: Context captured after the failed attempt.
    /// - `retry_block_reason`: Terminal reason that prevents another attempt.
    /// - `flow_started_at`: Monotonic instant when retry flow execution started.
    ///
    /// # Returns
    /// A retry action selected from listeners and configured limits.
    pub(in crate::executor) fn handle_failure(
        &self,
        attempts: u32,
        failure: AttemptFailure<E>,
        context: RetryContext,
        retry_block_reason: Option<RetryErrorReason>,
        flow_started_at: Instant,
    ) -> RetryFlowAction<E> {
        let hint = self
            .retry_after_hint
            .as_ref()
            .and_then(|hint| self.invoke_listener(|| hint.apply(&failure, &context)));
        let context = context
            .with_retry_after_hint(hint)
            .with_total_elapsed(flow_started_at.elapsed());

        let decision =
            self.resolve_failure_decision(self.failure_decision(&failure, &context), &failure);
        let context = context.with_total_elapsed(flow_started_at.elapsed());
        if decision == AttemptFailureDecision::Abort {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::Aborted,
                Some(failure),
                context,
            ));
        }

        if let Some(reason) = retry_block_reason {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        let max_attempts = self.options.max_attempts.get();
        if attempts >= max_attempts {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::AttemptsExceeded,
                Some(failure),
                context,
            ));
        }

        if let Some(reason) =
            self.elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        let delay = self.retry_delay(decision, attempts, hint);
        let context = context
            .with_total_elapsed(flow_started_at.elapsed())
            .with_next_delay(delay);
        if self.retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay) {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        self.emit_retry_scheduled(&failure, &context);
        let context = context.with_total_elapsed(flow_started_at.elapsed());
        if let Some(reason) =
            self.elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }
        if self.retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay) {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        RetryFlowAction::Retry { delay, failure }
    }

    /// Resolves all failure listeners into one decision.
    ///
    /// # Parameters
    /// - `failure`: Attempt failure.
    /// - `context`: Failure context.
    ///
    /// # Returns
    /// Last non-default listener decision, or [`AttemptFailureDecision::UseDefault`].
    fn failure_decision(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> AttemptFailureDecision {
        let mut decision = AttemptFailureDecision::UseDefault;
        for listener in &self.listeners.failure {
            let current = self.invoke_listener(|| listener.apply(failure, context));
            if current != AttemptFailureDecision::UseDefault {
                decision = current;
            }
        }
        decision
    }

    /// Resolves the effective failure decision after applying timeout policy.
    ///
    /// # Parameters
    /// - `decision`: Decision returned by failure listeners.
    /// - `failure`: Attempt failure being handled.
    ///
    /// # Returns
    /// A concrete decision for timeout failures when listeners used the default.
    fn resolve_failure_decision(
        &self,
        decision: AttemptFailureDecision,
        failure: &AttemptFailure<E>,
    ) -> AttemptFailureDecision {
        if decision != AttemptFailureDecision::UseDefault {
            return decision;
        }
        if matches!(failure, AttemptFailure::Timeout)
            && let Some(attempt_timeout) = self.options.attempt_timeout()
        {
            match attempt_timeout.policy() {
                AttemptTimeoutPolicy::Retry => AttemptFailureDecision::Retry,
                AttemptTimeoutPolicy::Abort => AttemptFailureDecision::Abort,
            }
        } else if matches!(
            failure,
            AttemptFailure::Panic(_) | AttemptFailure::Executor(_)
        ) {
            AttemptFailureDecision::Abort
        } else {
            AttemptFailureDecision::UseDefault
        }
    }

    /// Selects the delay used before the next retry.
    ///
    /// # Parameters
    /// - `decision`: Failure decision.
    /// - `attempts`: Attempts executed so far.
    /// - `hint`: Optional retry-after hint.
    ///
    /// # Returns
    /// Delay before the next retry.
    fn retry_delay(
        &self,
        decision: AttemptFailureDecision,
        attempts: u32,
        hint: Option<Duration>,
    ) -> Duration {
        match decision {
            AttemptFailureDecision::RetryAfter(delay) => delay,
            AttemptFailureDecision::UseDefault => hint.unwrap_or_else(|| {
                self.options
                    .jitter
                    .delay_for_attempt(&self.options.delay, attempts)
            }),
            AttemptFailureDecision::Retry | AttemptFailureDecision::Abort => self
                .options
                .jitter
                .delay_for_attempt(&self.options.delay, attempts),
        }
    }

    /// Builds an elapsed-budget error.
    ///
    /// # Parameters
    /// - `reason`: Elapsed-budget reason selected by the caller.
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed by this flow.
    /// - `attempts`: Attempts executed so far.
    /// - `last_failure`: Last observed failure, if any.
    /// - `attempt_timeout`: Timeout visible in the terminal context.
    ///
    /// # Returns
    /// A retry error preserving the terminal context.
    fn elapsed_error(
        &self,
        reason: RetryErrorReason,
        operation_elapsed: Duration,
        total_elapsed: Duration,
        attempts: u32,
        last_failure: Option<AttemptFailure<E>>,
        attempt_timeout: EffectiveAttemptTimeout,
    ) -> RetryError<E> {
        RetryError::new(
            reason,
            last_failure,
            self.context(
                operation_elapsed,
                total_elapsed,
                attempts,
                Duration::ZERO,
                attempt_timeout.duration,
            )
            .with_attempt_timeout_source(attempt_timeout.source),
        )
    }

    /// Returns the first elapsed-budget reason that is exhausted.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed by this flow.
    ///
    /// # Returns
    /// `Some(RetryErrorReason)` when an elapsed budget has been exhausted.
    #[inline]
    fn elapsed_error_reason(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> Option<RetryErrorReason> {
        if self
            .options
            .max_operation_elapsed
            .is_some_and(|max_operation_elapsed| operation_elapsed >= max_operation_elapsed)
        {
            Some(RetryErrorReason::MaxOperationElapsedExceeded)
        } else if self
            .options
            .max_total_elapsed
            .is_some_and(|max_total_elapsed| total_elapsed >= max_total_elapsed)
        {
            Some(RetryErrorReason::MaxTotalElapsedExceeded)
        } else {
            None
        }
    }

    /// Returns whether a selected retry sleep would consume the remaining total budget.
    ///
    /// # Parameters
    /// - `total_elapsed`: Total monotonic retry-flow time consumed before sleep.
    /// - `delay`: Selected retry delay.
    ///
    /// # Returns
    /// `true` when the delay should not be slept because no budget would remain
    /// for the next attempt.
    #[inline]
    fn retry_sleep_exhausts_total_elapsed(&self, total_elapsed: Duration, delay: Duration) -> bool {
        if delay.is_zero() {
            return false;
        }
        let Some(max_total_elapsed) = self.options.max_total_elapsed else {
            return false;
        };
        total_elapsed.saturating_add(delay) >= max_total_elapsed
    }

    /// Emits before-attempt listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    fn emit_before_attempt(&self, context: &RetryContext) {
        for listener in &self.listeners.before_attempt {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits attempt-success listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    pub(in crate::executor) fn emit_attempt_success(&self, context: &RetryContext) {
        for listener in &self.listeners.attempt_success {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits retry-scheduled listeners.
    ///
    /// # Parameters
    /// - `failure`: Failure that caused the retry to be scheduled.
    /// - `context`: Context carrying the selected next delay.
    fn emit_retry_scheduled(&self, failure: &AttemptFailure<E>, context: &RetryContext) {
        for listener in &self.listeners.retry_scheduled {
            self.invoke_listener(|| {
                listener.accept(failure, context);
            });
        }
    }

    /// Emits terminal error listeners and returns the same error.
    ///
    /// # Parameters
    /// - `error`: Terminal retry error.
    ///
    /// # Returns
    /// The same error after listeners have been invoked.
    pub(in crate::executor) fn emit_error(&self, error: RetryError<E>) -> RetryError<E> {
        for listener in &self.listeners.error {
            self.invoke_listener(|| {
                listener.accept(&error, error.context());
            });
        }
        error
    }

    /// Invokes a listener and optionally isolates panics.
    ///
    /// # Parameters
    /// - `call`: Listener invocation closure.
    ///
    /// # Returns
    /// The listener return value, or `Default::default()` when an isolated panic
    /// occurs.
    fn invoke_listener<R>(&self, call: impl FnOnce() -> R) -> R
    where
        R: Default,
    {
        if self.isolate_listener_panics {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_default()
        } else {
            call()
        }
    }
}

impl<E> fmt::Debug for Retry<E> {
    /// Formats the retry policy without exposing callbacks.
    ///
    /// # Parameters
    /// - `f`: Formatter.
    ///
    /// # Returns
    /// Formatter result.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Retry")
            .field("options", &self.options)
            .finish_non_exhaustive()
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
