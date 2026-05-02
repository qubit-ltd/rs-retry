/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry builder.
//!
//! The builder collects retry options, attempt listeners, failure listeners, and
//! terminal error listeners before producing a validated [`Retry`].

use std::time::Duration;

use qubit_error::BoxError;
use qubit_function::{BiConsumer, BiFunction, BiPredicate, Consumer};

use crate::constants::KEY_MAX_ATTEMPTS;
use crate::event::RetryListeners;
use crate::{
    AttemptFailure, AttemptFailureDecision, AttemptTimeoutOption, AttemptTimeoutPolicy, Retry,
    RetryAfterHint, RetryConfigError, RetryContext, RetryDelay, RetryError, RetryJitter,
    RetryOptions,
};

/// Builder for [`Retry`].
///
/// The generic parameter `E` is the operation error type preserved inside
/// [`AttemptFailure::Error`]. Failure listeners may observe failures, override
/// the retry decision, or return [`AttemptFailureDecision::UseDefault`] to let
/// the policy decide from configured limits and delay strategy.
pub struct RetryBuilder<E = BoxError> {
    /// Retry limits, delay strategy, jitter, and elapsed budgets.
    options: RetryOptions,
    /// Pending policy used when timeout duration is configured later.
    pending_attempt_timeout_policy: AttemptTimeoutPolicy,
    /// Optional retry-after hint extractor.
    retry_after_hint: Option<RetryAfterHint<E>>,
    /// Lifecycle listeners registered on the builder.
    listeners: RetryListeners<E>,
    /// Whether listener panics should be isolated.
    isolate_listener_panics: bool,
    /// Stored validation error when max attempts is configured as zero.
    max_attempts_error: Option<RetryConfigError>,
}

impl<E> RetryBuilder<E> {
    /// Creates a builder with default options and no listeners.
    ///
    /// # Returns
    /// A retry builder using [`RetryOptions::default`].
    #[inline]
    pub fn new() -> Self {
        Self {
            options: RetryOptions::default(),
            pending_attempt_timeout_policy: AttemptTimeoutPolicy::default(),
            retry_after_hint: None,
            listeners: RetryListeners::default(),
            isolate_listener_panics: false,
            max_attempts_error: None,
        }
    }

    /// Replaces all retry options.
    ///
    /// # Parameters
    /// - `options`: Retry option snapshot.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn options(mut self, options: RetryOptions) -> Self {
        self.pending_attempt_timeout_policy = options
            .attempt_timeout()
            .map(|attempt_timeout| attempt_timeout.policy())
            .unwrap_or_default();
        self.options = options;
        self.max_attempts_error = None;
        self
    }

    /// Sets the maximum total attempts, including the initial attempt.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum attempts. Zero is recorded as a build error.
    ///
    /// # Returns
    /// The updated builder.
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        if let Some(max_attempts) = std::num::NonZeroU32::new(max_attempts) {
            self.options.max_attempts = max_attempts;
            self.max_attempts_error = None;
        } else {
            self.max_attempts_error = Some(RetryConfigError::invalid_value(
                KEY_MAX_ATTEMPTS,
                "max_attempts must be greater than zero",
            ));
        }
        self
    }

    /// Sets the maximum retry count after the initial attempt.
    ///
    /// # Parameters
    /// - `max_retries`: Number of retries after the first attempt.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn max_retries(self, max_retries: u32) -> Self {
        self.max_attempts(max_retries.saturating_add(1))
    }

    /// Sets the maximum cumulative user operation time.
    ///
    /// # Parameters
    /// - `max_operation_elapsed`: Optional cumulative user operation time budget.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn max_operation_elapsed(mut self, max_operation_elapsed: Option<Duration>) -> Self {
        self.options.max_operation_elapsed = max_operation_elapsed;
        self
    }

    /// Sets the maximum total monotonic retry-flow elapsed time.
    ///
    /// # Parameters
    /// - `max_total_elapsed`: Optional total retry-flow time budget. Operation
    ///   execution, retry sleeps, retry-after sleeps, and retry control-path
    ///   listener time are included.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn max_total_elapsed(mut self, max_total_elapsed: Option<Duration>) -> Self {
        self.options.max_total_elapsed = max_total_elapsed;
        self
    }

    /// Sets the retry delay strategy.
    ///
    /// # Parameters
    /// - `delay`: Base delay strategy used between attempts.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn delay(mut self, delay: RetryDelay) -> Self {
        self.options.delay = delay;
        self
    }

    /// Configures immediate retries with no sleep.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn no_delay(self) -> Self {
        self.delay(RetryDelay::none())
    }

    /// Configures a fixed retry delay.
    ///
    /// # Parameters
    /// - `delay`: Delay slept before each retry.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn fixed_delay(self, delay: Duration) -> Self {
        self.delay(RetryDelay::fixed(delay))
    }

    /// Configures a random retry delay range.
    ///
    /// # Parameters
    /// - `min`: Inclusive lower delay bound.
    /// - `max`: Inclusive upper delay bound.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn random_delay(self, min: Duration, max: Duration) -> Self {
        self.delay(RetryDelay::random(min, max))
    }

    /// Configures exponential backoff with the default multiplier `2.0`.
    ///
    /// # Parameters
    /// - `initial`: First retry delay.
    /// - `max`: Maximum retry delay.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn exponential_backoff(self, initial: Duration, max: Duration) -> Self {
        self.exponential_backoff_with_multiplier(initial, max, 2.0)
    }

    /// Configures exponential backoff with a custom multiplier.
    ///
    /// # Parameters
    /// - `initial`: First retry delay.
    /// - `max`: Maximum retry delay.
    /// - `multiplier`: Multiplier applied after each failed attempt.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn exponential_backoff_with_multiplier(
        self,
        initial: Duration,
        max: Duration,
        multiplier: f64,
    ) -> Self {
        self.delay(RetryDelay::exponential(initial, max, multiplier))
    }

    /// Sets the jitter strategy.
    ///
    /// # Parameters
    /// - `jitter`: Jitter strategy applied to base delays.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn jitter(mut self, jitter: RetryJitter) -> Self {
        self.options.jitter = jitter;
        self
    }

    /// Sets relative jitter by factor.
    ///
    /// # Parameters
    /// - `factor`: Relative jitter factor in `[0.0, 1.0]`.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn jitter_factor(self, factor: f64) -> Self {
        self.jitter(RetryJitter::factor(factor))
    }

    /// Sets a per-attempt timeout.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Timeout applied by `run_async`, `run_in_worker`,
    ///   and `run_blocking_with_timeout`. `None` disables per-attempt timeout.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout(mut self, attempt_timeout: Option<Duration>) -> Self {
        if let Some(timeout) = attempt_timeout {
            self.options.attempt_timeout = Some(AttemptTimeoutOption::new(
                timeout,
                self.pending_attempt_timeout_policy,
            ));
        } else {
            self.pending_attempt_timeout_policy = AttemptTimeoutPolicy::default();
            self.options.attempt_timeout = None;
        }
        self
    }

    /// Sets the complete per-attempt timeout option.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Timeout option. `None` disables per-attempt timeout.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout_option(mut self, attempt_timeout: Option<AttemptTimeoutOption>) -> Self {
        if let Some(attempt_timeout) = attempt_timeout {
            self.pending_attempt_timeout_policy = attempt_timeout.policy();
        } else {
            self.pending_attempt_timeout_policy = AttemptTimeoutPolicy::default();
        }
        self.options.attempt_timeout = attempt_timeout;
        self
    }

    /// Sets the policy used when an attempt times out.
    ///
    /// If a timeout duration is already configured, this updates the complete
    /// timeout option. Otherwise the policy is kept and applied when
    /// [`RetryBuilder::attempt_timeout`] is called later.
    ///
    /// # Parameters
    /// - `policy`: Timeout policy to use.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout_policy(mut self, policy: AttemptTimeoutPolicy) -> Self {
        self.pending_attempt_timeout_policy = policy;
        self.options.attempt_timeout = self
            .options
            .attempt_timeout
            .map(|attempt_timeout| attempt_timeout.with_policy(policy));
        self
    }

    /// Sets how long worker-thread execution waits after cancelling a timed-out worker.
    ///
    /// # Parameters
    /// - `grace`: Duration to wait after the attempt timeout fires and the
    ///   cooperative cancellation token is marked as cancelled. Use zero to skip
    ///   the grace wait.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn worker_cancel_grace(mut self, grace: Duration) -> Self {
        self.options.worker_cancel_grace = grace;
        self
    }

    /// Extracts an optional retry-after hint from each failure.
    ///
    /// # Parameters
    /// - `hint`: Function that inspects a failure and context before failure
    ///   listeners run.
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_after_hint<H>(mut self, hint: H) -> Self
    where
        H: BiFunction<AttemptFailure<E>, RetryContext, Option<Duration>> + Send + Sync + 'static,
    {
        self.retry_after_hint = Some(hint.into_arc());
        self
    }

    /// Extracts an optional retry-after hint from operation errors.
    ///
    /// # Parameters
    /// - `hint`: Function returning a delay hint for application errors.
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_after_from_error<H>(self, hint: H) -> Self
    where
        H: Fn(&E) -> Option<Duration> + Send + Sync + 'static,
    {
        self.retry_after_hint(
            move |failure: &AttemptFailure<E>, _context: &RetryContext| {
                failure.as_error().and_then(&hint)
            },
        )
    }

    /// Registers a listener invoked before every attempt.
    ///
    /// # Parameters
    /// - `listener`: Listener receiving the retry context.
    ///
    /// # Returns
    /// The updated builder.
    pub fn before_attempt<C>(mut self, listener: C) -> Self
    where
        C: Consumer<RetryContext> + Send + Sync + 'static,
    {
        self.listeners.before_attempt.push(listener.into_arc());
        self
    }

    /// Registers a listener invoked when an attempt succeeds.
    ///
    /// # Parameters
    /// - `listener`: Listener receiving the success context.
    ///
    /// # Returns
    /// The updated builder.
    pub fn on_success<C>(mut self, listener: C) -> Self
    where
        C: Consumer<RetryContext> + Send + Sync + 'static,
    {
        self.listeners.attempt_success.push(listener.into_arc());
        self
    }

    /// Registers a listener invoked after each attempt failure.
    ///
    /// # Parameters
    /// - `listener`: Listener returning a retry failure decision.
    ///
    /// # Returns
    /// The updated builder.
    pub fn on_failure<F>(mut self, listener: F) -> Self
    where
        F: BiFunction<AttemptFailure<E>, RetryContext, AttemptFailureDecision>
            + Send
            + Sync
            + 'static,
    {
        self.listeners.failure.push(listener.into_arc());
        self
    }

    /// Registers a listener invoked after a retry delay has been selected.
    ///
    /// The listener receives the failed attempt and a context whose
    /// [`RetryContext::next_delay`] contains the delay that will be slept before
    /// the next attempt. The listener is observational and cannot change the
    /// retry decision.
    ///
    /// # Parameters
    /// - `listener`: Listener receiving the failure and scheduled-retry context.
    ///
    /// # Returns
    /// The updated builder.
    pub fn on_retry<C>(mut self, listener: C) -> Self
    where
        C: BiConsumer<AttemptFailure<E>, RetryContext> + Send + Sync + 'static,
    {
        self.listeners.retry_scheduled.push(listener.into_arc());
        self
    }

    /// Registers an error-only predicate where `true` means retry.
    ///
    /// # Parameters
    /// - `predicate`: Predicate applied only to [`AttemptFailure::Error`].
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_if_error<P>(self, predicate: P) -> Self
    where
        P: BiPredicate<E, RetryContext> + Send + Sync + 'static,
    {
        self.on_failure(
            move |failure: &AttemptFailure<E>, context: &RetryContext| match failure {
                AttemptFailure::Error(error) => {
                    if predicate.test(error, context) {
                        AttemptFailureDecision::Retry
                    } else {
                        AttemptFailureDecision::Abort
                    }
                }
                AttemptFailure::Timeout
                | AttemptFailure::Panic(_)
                | AttemptFailure::Executor(_) => AttemptFailureDecision::UseDefault,
            },
        )
    }

    /// Registers a listener invoked when the retry flow returns [`RetryError`].
    ///
    /// # Parameters
    /// - `listener`: Observational listener that cannot resume the retry flow.
    ///
    /// # Returns
    /// The updated builder.
    pub fn on_error<C>(mut self, listener: C) -> Self
    where
        C: BiConsumer<RetryError<E>, RetryContext> + Send + Sync + 'static,
    {
        self.listeners.error.push(listener.into_arc());
        self
    }

    /// Aborts the retry flow when a configured per-attempt timeout expires.
    ///
    /// Max-elapsed effective timeouts are not controlled by this policy and stop
    /// with [`crate::RetryErrorReason::MaxOperationElapsedExceeded`].
    ///
    /// # Returns
    /// The updated builder.
    pub fn abort_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
    }

    /// Retries configured per-attempt timeouts while limits allow it.
    ///
    /// Max-elapsed effective timeouts are not controlled by this policy and stop
    /// with [`crate::RetryErrorReason::MaxOperationElapsedExceeded`].
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Retry)
    }

    /// Enables panic isolation for all registered listeners.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn isolate_listener_panics(mut self) -> Self {
        self.isolate_listener_panics = true;
        self
    }

    /// Builds and validates the retry policy.
    ///
    /// # Returns
    /// A validated [`Retry`].
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when options are invalid.
    pub fn build(self) -> Result<Retry<E>, RetryConfigError> {
        if let Some(error) = self.max_attempts_error {
            return Err(error);
        }
        self.options.validate()?;
        Ok(Retry::new(
            self.options,
            self.retry_after_hint,
            self.isolate_listener_panics,
            self.listeners,
        ))
    }
}

impl<E> Default for RetryBuilder<E> {
    /// Creates a default retry builder.
    ///
    /// # Returns
    /// A builder equivalent to [`RetryBuilder::new`].
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
