/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry builder.
//!
//! The builder collects retry options, attempt listeners, failure listeners, and
//! terminal error listeners before producing a validated [`Retry`].

use std::time::Duration;

use qubit_common::BoxError;
use qubit_function::{BiConsumer, BiFunction, BiPredicate, Consumer};

use crate::constants::KEY_MAX_ATTEMPTS;
use crate::event::RetryListeners;
use crate::{
    AttemptFailure, AttemptFailureDecision, Retry, RetryAfterHint, RetryConfigError, RetryContext,
    RetryDelay, RetryError, RetryJitter, RetryOptions,
};

/// Builder for [`Retry`].
///
/// The generic parameter `E` is the operation error type preserved inside
/// [`AttemptFailure::Error`]. Failure listeners may observe failures, override the
/// retry decision, or return [`AttemptFailureDecision::UseDefault`] to let the
/// policy decide from configured limits and delay strategy.
pub struct RetryBuilder<E = BoxError> {
    /// Retry limits, delay strategy, jitter, and elapsed-time budget.
    options: RetryOptions,
    /// Per-attempt timeout used by async execution.
    attempt_timeout: Option<Duration>,
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
            attempt_timeout: None,
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

    /// Sets the maximum total elapsed time.
    ///
    /// # Parameters
    /// - `max_elapsed`: Optional total elapsed-time budget.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn max_elapsed(mut self, max_elapsed: Option<Duration>) -> Self {
        self.options.max_elapsed = max_elapsed;
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

    /// Sets an async per-attempt timeout.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Timeout applied to each async attempt. `None`
    ///   disables per-attempt timeout.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout(mut self, attempt_timeout: Option<Duration>) -> Self {
        self.attempt_timeout = attempt_timeout;
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
                AttemptFailure::Timeout => AttemptFailureDecision::UseDefault,
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

    /// Aborts the retry flow when an attempt times out.
    ///
    /// # Returns
    /// The updated builder.
    pub fn abort_on_timeout(self) -> Self {
        self.on_failure(
            |failure: &AttemptFailure<E>, _context: &RetryContext| match failure {
                AttemptFailure::Timeout => AttemptFailureDecision::Abort,
                AttemptFailure::Error(_) => AttemptFailureDecision::UseDefault,
            },
        )
    }

    /// Retries timed-out attempts while limits allow it.
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_on_timeout(self) -> Self {
        self.on_failure(
            |failure: &AttemptFailure<E>, _context: &RetryContext| match failure {
                AttemptFailure::Timeout => AttemptFailureDecision::Retry,
                AttemptFailure::Error(_) => AttemptFailureDecision::UseDefault,
            },
        )
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
            self.attempt_timeout,
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
