// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry builder.
//!
//! The builder collects retry options, attempt listeners, failure listeners,
//! and terminal error listeners before producing a validated [`Retry`].
//! It is the main public configuration surface; once [`RetryBuilder::build`]
//! succeeds, the resulting policy is immutable and can be cloned cheaply.

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::StdTimer;
use qubit_clock::Timer;
use qubit_error::BoxError;
use qubit_function::ArcBiConsumer;
use qubit_function::ArcBiFunction;
use qubit_function::ArcConsumer;
use qubit_function::BiConsumer;
use qubit_function::BiFunction;
use qubit_function::BiPredicate;
use qubit_function::Consumer;

use crate::AttemptFailure;
use crate::AttemptFailureDecision;
use crate::AttemptTimeoutOption;
use crate::AttemptTimeoutPolicy;
use crate::Retry;
use crate::RetryAfterPolicy;
use crate::RetryConfigError;
use crate::RetryContext;
use crate::RetryDelay;
use crate::RetryError;
use crate::RetryJitter;
use crate::RetryOptions;
use crate::RetryRandomSource;
use crate::constants::KEY_MAX_ATTEMPTS;
use crate::event::RetryAfterHint;
use crate::event::RetryListeners;
use crate::random::ThreadRetryRandomSource;

/// Builder for [`Retry`].
///
/// The generic parameter `E` is the operation error type preserved inside
/// [`AttemptFailure::Error`]. Failure listeners may observe failures, override
/// the retry decision, or return [`AttemptFailureDecision::UseDefault`] to let
/// the policy decide from configured limits and delay strategy.
///
/// # Examples
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// use qubit_retry::Retry;
///
/// Retry::<&'static str>::builder();
/// ```
#[must_use]
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
    /// Random source used by delay and jitter strategies.
    random_source: Arc<dyn RetryRandomSource>,
    /// Timer used by sync and worker execution.
    blocking_timer: Arc<dyn Timer>,
    /// Optional caller-supplied timer used by Tokio async execution.
    #[cfg(feature = "tokio")]
    async_timer: Option<Arc<dyn Timer>>,
}

impl<E> RetryBuilder<E> {
    /// Creates a builder with default options and no listeners.
    ///
    /// With the `tokio` feature enabled, the default async clock and timer
    /// are created when the built policy's `Retry::run_async` future is first
    /// polled. Constructing the builder does not bind it to a Tokio runtime.
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
            random_source: Arc::new(ThreadRetryRandomSource),
            blocking_timer: Arc::new(StdTimer::new()),
            #[cfg(feature = "tokio")]
            async_timer: None,
        }
    }

    /// Sets the random source used by delay and jitter strategies.
    ///
    /// The source is shared by every execution of the built policy and must
    /// therefore provide its own synchronization when it stores mutable state.
    ///
    /// # Parameters
    ///
    /// * `random_source` - Shared source used for random delay and jitter
    ///   sampling.
    ///
    /// # Returns
    ///
    /// The updated builder.
    #[inline(always)]
    pub fn random_source(
        mut self,
        random_source: Arc<dyn RetryRandomSource>,
    ) -> Self {
        self.random_source = random_source;
        self
    }

    /// Sets the timer for sync and worker execution.
    ///
    /// The same object measures operation and total elapsed time and performs
    /// retry backoff waits. Supplying a manual timer therefore makes both
    /// elapsed budgets and retry delays deterministic. Because sync and worker
    /// runners park the calling thread, the timer backend must continue making
    /// progress independently while a retry delay is pending.
    ///
    /// # Arguments
    /// - `timer`: Shared timer for [`Retry::run`] and [`Retry::run_in_worker`].
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn blocking_timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.blocking_timer = timer;
        self
    }

    /// Sets the timer and monotonic clock for Tokio async execution.
    ///
    /// The same object measures operation and total elapsed time, enforces
    /// async attempt timeouts, and performs retry backoff waits. The timer's
    /// clock and deadline driver must remain alive and progressing for the
    /// retry future's lifetime. A Tokio timer retains its target runtime handle
    /// and may be polled from another runtime context.
    ///
    /// # Arguments
    /// - `timer`: Shared timer for [`Retry::run_async`].
    ///
    /// # Returns
    /// The updated builder.
    #[cfg(feature = "tokio")]
    #[inline(always)]
    pub fn async_timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.async_timer = Some(timer);
        self
    }

    /// Replaces all retry options.
    ///
    /// # Arguments
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
    /// # Arguments
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
    /// # Arguments
    /// - `max_retries`: Number of retries after the first attempt.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn max_retries(self, max_retries: u32) -> Self {
        self.max_attempts(max_retries.saturating_add(1))
    }

    /// Sets the maximum cumulative user operation time.
    ///
    /// # Arguments
    /// - `max_operation_elapsed`: Optional cumulative user operation time
    ///   budget.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn max_operation_elapsed(
        mut self,
        max_operation_elapsed: Option<Duration>,
    ) -> Self {
        self.options.max_operation_elapsed = max_operation_elapsed;
        self
    }

    /// Sets the maximum total monotonic retry-flow elapsed time.
    ///
    /// # Arguments
    /// - `max_total_elapsed`: Optional total retry-flow time budget. Operation
    ///   execution, retry sleeps, retry-after sleeps, and retry control-path
    ///   listener time are included.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn max_total_elapsed(
        mut self,
        max_total_elapsed: Option<Duration>,
    ) -> Self {
        self.options.max_total_elapsed = max_total_elapsed;
        self
    }

    /// Sets the retry delay strategy.
    ///
    /// # Arguments
    /// - `delay`: Base delay strategy used between attempts.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn delay(mut self, delay: RetryDelay) -> Self {
        self.options.delay = delay;
        self
    }

    /// Configures immediate retries with no sleep.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn no_delay(self) -> Self {
        self.delay(RetryDelay::none())
    }

    /// Configures a fixed retry delay.
    ///
    /// # Arguments
    /// - `delay`: Delay slept before each retry.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn fixed_delay(self, delay: Duration) -> Self {
        self.delay(RetryDelay::fixed(delay))
    }

    /// Configures a random retry delay range.
    ///
    /// # Arguments
    /// - `min`: Inclusive lower delay bound.
    /// - `max`: Inclusive upper delay bound.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn random_delay(self, min: Duration, max: Duration) -> Self {
        self.delay(RetryDelay::random(min, max))
    }

    /// Configures exponential backoff with the default multiplier `2.0`.
    ///
    /// # Arguments
    /// - `initial`: First retry delay.
    /// - `max`: Maximum retry delay.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn exponential_backoff(self, initial: Duration, max: Duration) -> Self {
        self.exponential_backoff_with_multiplier(initial, max, 2.0)
    }

    /// Configures exponential backoff with a custom multiplier.
    ///
    /// # Arguments
    /// - `initial`: First retry delay.
    /// - `max`: Maximum retry delay.
    /// - `multiplier`: Multiplier applied after each failed attempt.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
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
    /// # Arguments
    /// - `jitter`: Jitter strategy applied to base delays.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn jitter(mut self, jitter: RetryJitter) -> Self {
        self.options.jitter = jitter;
        self
    }

    /// Sets relative jitter by factor.
    ///
    /// # Arguments
    /// - `factor`: Relative jitter factor in `[0.0, 1.0]`.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn jitter_factor(self, factor: f64) -> Self {
        self.jitter(RetryJitter::factor(factor))
    }

    /// Sets a per-attempt timeout.
    ///
    /// # Arguments
    /// - `attempt_timeout`: Timeout applied by `run_async` and `run_in_worker`.
    ///   `None` disables per-attempt timeout.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout(
        mut self,
        attempt_timeout: Option<Duration>,
    ) -> Self {
        if let Some(timeout) = attempt_timeout {
            self.options.attempt_timeout = Some(AttemptTimeoutOption::new(
                timeout,
                self.pending_attempt_timeout_policy,
            ));
        } else {
            self.pending_attempt_timeout_policy =
                AttemptTimeoutPolicy::default();
            self.options.attempt_timeout = None;
        }
        self
    }

    /// Sets the complete per-attempt timeout option.
    ///
    /// # Arguments
    /// - `attempt_timeout`: Timeout option. `None` disables per-attempt
    ///   timeout.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout_option(
        mut self,
        attempt_timeout: Option<AttemptTimeoutOption>,
    ) -> Self {
        if let Some(attempt_timeout) = attempt_timeout {
            self.pending_attempt_timeout_policy = attempt_timeout.policy();
        } else {
            self.pending_attempt_timeout_policy =
                AttemptTimeoutPolicy::default();
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
    /// # Arguments
    /// - `policy`: Timeout policy to use.
    ///
    /// # Returns
    /// The updated builder.
    #[inline]
    pub fn attempt_timeout_policy(
        mut self,
        policy: AttemptTimeoutPolicy,
    ) -> Self {
        self.pending_attempt_timeout_policy = policy;
        self.options.attempt_timeout = self
            .options
            .attempt_timeout
            .map(|attempt_timeout| attempt_timeout.with_policy(policy));
        self
    }

    /// Sets how long worker-thread execution waits after cancelling a timed-out
    /// worker.
    ///
    /// # Arguments
    /// - `grace`: Duration to wait after the attempt timeout fires and the
    ///   cooperative cancellation token is marked as cancelled. Use zero to
    ///   skip the grace wait.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn worker_cancel_grace(mut self, grace: Duration) -> Self {
        self.options.worker_cancel_grace = grace;
        self
    }

    /// Sets how Retry-After hints combine with configured delays.
    pub fn retry_after_policy(mut self, policy: RetryAfterPolicy) -> Self {
        self.options.retry_after_policy = policy;
        self
    }

    /// Extracts an optional retry-after hint from each failure.
    ///
    /// # Arguments
    /// - `hint`: Function that inspects a failure and context before failure
    ///   listeners run.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn retry_after_hint<H>(mut self, hint: H) -> Self
    where
        H: BiFunction<AttemptFailure<E>, RetryContext, Option<Duration>>
            + Send
            + Sync
            + 'static,
    {
        self.retry_after_hint = Some(ArcBiFunction::new(hint));
        self
    }

    /// Extracts an optional retry-after hint from operation errors.
    ///
    /// # Arguments
    /// - `hint`: Function returning a delay hint for application errors.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
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
    /// # Arguments
    /// - `listener`: Listener receiving the retry context.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn before_attempt<C>(mut self, listener: C) -> Self
    where
        C: Consumer<RetryContext> + Send + Sync + 'static,
    {
        self.listeners
            .before_attempt
            .push(ArcConsumer::new(listener));
        self
    }

    /// Registers a listener invoked when an attempt succeeds.
    ///
    /// # Arguments
    /// - `listener`: Listener receiving the success context.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn on_success<C>(mut self, listener: C) -> Self
    where
        C: Consumer<RetryContext> + Send + Sync + 'static,
    {
        self.listeners
            .attempt_success
            .push(ArcConsumer::new(listener));
        self
    }

    /// Registers a listener invoked after each attempt failure.
    ///
    /// The listener observes every failure produced by an admitted attempt,
    /// after retry-after extraction has populated
    /// [`RetryContext::retry_after_hint`]. All registered failure listeners run
    /// once in registration order. Normally their returned decisions control
    /// abort, retry, and delay selection.
    ///
    /// A timeout caused by exhausted max-operation or max-total elapsed budget
    /// is a hard stop: failure listeners still observe it exactly once, but
    /// their decisions are ignored, and no retry-scheduled event is emitted.
    /// Terminal diagnostics not produced by an admitted attempt bypass failure
    /// listeners and are delivered to the terminal error listeners.
    ///
    /// # Arguments
    /// - `listener`: Listener returning a retry failure decision.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn on_failure<F>(mut self, listener: F) -> Self
    where
        F: BiFunction<AttemptFailure<E>, RetryContext, AttemptFailureDecision>
            + Send
            + Sync
            + 'static,
    {
        self.listeners.failure.push(ArcBiFunction::new(listener));
        self
    }

    /// Registers a listener invoked after a retry delay has been selected.
    ///
    /// The listener receives the failed attempt and a context whose
    /// [`RetryContext::next_delay`] contains the delay that will be slept
    /// before the next attempt. The listener is observational and cannot
    /// change the retry decision.
    ///
    /// # Arguments
    /// - `listener`: Listener receiving the failure and scheduled-retry
    ///   context.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn on_retry<C>(mut self, listener: C) -> Self
    where
        C: BiConsumer<AttemptFailure<E>, RetryContext> + Send + Sync + 'static,
    {
        self.listeners
            .retry_scheduled
            .push(ArcBiConsumer::new(listener));
        self
    }

    /// Registers an error-only predicate where `true` means retry.
    ///
    /// # Arguments
    /// - `predicate`: Predicate applied only to [`AttemptFailure::Error`].
    ///
    /// # Returns
    /// The updated builder.
    pub fn retry_if_error<P>(self, predicate: P) -> Self
    where
        P: BiPredicate<E, RetryContext> + Send + Sync + 'static,
    {
        self.on_failure(
            move |failure: &AttemptFailure<E>, context: &RetryContext| {
                match failure {
                    AttemptFailure::Error(error) => {
                        if predicate.test(error, context) {
                            AttemptFailureDecision::Retry
                        } else {
                            AttemptFailureDecision::Abort
                        }
                    }
                    AttemptFailure::Timeout
                    | AttemptFailure::Panic(_)
                    | AttemptFailure::Executor(_) => {
                        AttemptFailureDecision::UseDefault
                    }
                }
            },
        )
    }

    /// Registers a listener invoked when the retry flow returns [`RetryError`].
    ///
    /// # Arguments
    /// - `listener`: Observational listener that cannot resume the retry flow.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn on_error<C>(mut self, listener: C) -> Self
    where
        C: BiConsumer<RetryError<E>, RetryContext> + Send + Sync + 'static,
    {
        self.listeners.error.push(ArcBiConsumer::new(listener));
        self
    }

    /// Aborts the retry flow when a configured per-attempt timeout expires.
    ///
    /// Max-elapsed effective timeouts are not controlled by this policy and
    /// stop with [`crate::RetryErrorReason::MaxOperationElapsedExceeded`].
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn abort_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
    }

    /// Retries configured per-attempt timeouts while limits allow it.
    ///
    /// Max-elapsed effective timeouts are not controlled by this policy and
    /// stop with [`crate::RetryErrorReason::MaxOperationElapsedExceeded`].
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
    pub fn retry_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Retry)
    }

    /// Enables panic isolation for all registered listeners.
    ///
    /// # Returns
    /// The updated builder.
    #[inline(always)]
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
            self.random_source,
            self.blocking_timer,
            #[cfg(feature = "tokio")]
            self.async_timer,
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
