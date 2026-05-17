/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry option snapshot and configuration loading helpers.
//!
//! This module contains the immutable options consumed by [`crate::Retry`].
//! Raw config merge logic lives in [`crate::options::retry_config_values`].
//! Runtime runners should not interpret individual knobs directly when there is
//! combined behavior. Methods such as [`RetryOptions::effective_attempt_timeout`]
//! and [`RetryOptions::retry_delay`] encode the library's precedence rules in
//! one place.

use std::num::NonZeroU32;
use std::time::Duration;

#[cfg(feature = "config")]
use qubit_config::ConfigReader;

#[cfg(feature = "config")]
use super::retry_config_values::RetryConfigValues;
use super::{
    AttemptTimeoutOption,
    EffectiveAttemptTimeout,
};

use crate::constants::{
    DEFAULT_RETRY_MAX_ATTEMPTS,
    DEFAULT_RETRY_MAX_OPERATION_ELAPSED,
    DEFAULT_RETRY_MAX_TOTAL_ELAPSED,
    DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS,
    KEY_ATTEMPT_TIMEOUT_MILLIS,
    KEY_DELAY,
    KEY_JITTER_FACTOR,
    KEY_MAX_ATTEMPTS,
};
use crate::{
    AttemptFailureDecision,
    AttemptTimeoutSource,
    RetryConfigError,
    RetryDelay,
    RetryErrorReason,
    RetryJitter,
};

/// Immutable retry option snapshot used by [`crate::Retry`].
///
/// `RetryOptions` owns all executor configuration that is independent of the
/// application error type: attempt limits, elapsed budgets, delay strategy, and
/// jitter strategy. Construction validates the delay and jitter values before
/// an executor can use them.
///
#[derive(Debug, Clone, PartialEq)]
pub struct RetryOptions {
    /// Maximum attempts, including the initial attempt.
    pub(crate) max_attempts: NonZeroU32,
    /// Maximum cumulative user operation time for the retry flow.
    pub(crate) max_operation_elapsed: Option<Duration>,
    /// Maximum monotonic elapsed time for the whole retry flow.
    pub(crate) max_total_elapsed: Option<Duration>,
    /// Base delay strategy between attempts.
    pub(crate) delay: RetryDelay,
    /// RetryJitter applied to each base delay.
    pub(crate) jitter: RetryJitter,
    /// Optional per-attempt timeout settings.
    pub(crate) attempt_timeout: Option<AttemptTimeoutOption>,
    /// Grace period for a timed-out worker to observe cancellation and exit.
    pub(crate) worker_cancel_grace: Duration,
}

impl RetryOptions {
    /// Returns maximum attempts, including the initial attempt.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Maximum attempts configured for one retry execution.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts.get()
    }

    /// Returns maximum cumulative user operation time budget.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded executions, or `None` for unlimited.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns maximum total retry-flow elapsed time budget.
    ///
    /// This budget is measured with monotonic time and includes operation
    /// execution, retry sleeps, retry-after sleeps, and retry control-path
    /// listener time.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded executions, or `None` for unlimited.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }

    /// Returns the base delay strategy.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Borrowed delay strategy used by the executor.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn delay(&self) -> &RetryDelay {
        &self.delay
    }

    /// Returns the jitter strategy.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Jitter strategy used by the executor.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn jitter(&self) -> RetryJitter {
        self.jitter
    }

    /// Returns the optional per-attempt timeout settings.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(AttemptTimeoutOption)` when per-attempt timeout is configured.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn attempt_timeout(&self) -> Option<AttemptTimeoutOption> {
        self.attempt_timeout
    }

    /// Returns the worker cancellation grace period.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Duration the worker-thread executor waits after requesting cooperative
    /// cancellation for a timed-out worker attempt.
    #[inline]
    pub fn worker_cancel_grace(&self) -> Duration {
        self.worker_cancel_grace
    }

    /// Creates and validates a retry option snapshot.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum number of attempts, including the first call.
    ///   Must be greater than zero.
    /// - `max_operation_elapsed`: Optional cumulative user operation time budget for all
    ///   attempts. Listener execution and retry sleeps are excluded.
    /// - `max_total_elapsed`: Optional monotonic elapsed-time budget for the
    ///   whole retry flow. Operation execution, retry sleeps, retry-after
    ///   sleeps, and retry control-path listener time are included.
    /// - `delay`: Base delay strategy used between attempts.
    /// - `jitter`: RetryJitter strategy applied to each base delay.
    ///
    /// # Returns
    /// A validated [`RetryOptions`] value.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when `max_attempts` is zero, or when
    /// `delay` or `jitter` contains invalid parameters.
    pub fn new(
        max_attempts: u32,
        max_operation_elapsed: Option<Duration>,
        max_total_elapsed: Option<Duration>,
        delay: RetryDelay,
        jitter: RetryJitter,
    ) -> Result<Self, RetryConfigError> {
        Self::new_with_attempt_timeout(
            max_attempts,
            max_operation_elapsed,
            max_total_elapsed,
            delay,
            jitter,
            None,
        )
    }

    /// Creates and validates a retry option snapshot with attempt timeout.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum number of attempts, including the first call.
    ///   Must be greater than zero.
    /// - `max_operation_elapsed`: Optional cumulative user operation time budget for all
    ///   attempts. Listener execution and retry sleeps are excluded.
    /// - `max_total_elapsed`: Optional monotonic elapsed-time budget for the
    ///   whole retry flow. Operation execution, retry sleeps, retry-after
    ///   sleeps, and retry control-path listener time are included.
    /// - `delay`: Base delay strategy used between attempts.
    /// - `jitter`: RetryJitter strategy applied to each base delay.
    /// - `attempt_timeout`: Optional per-attempt timeout settings.
    ///
    /// # Returns
    /// A validated [`RetryOptions`] value.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when `max_attempts` is zero, when delay or
    /// jitter contains invalid parameters, or when the attempt timeout is zero.
    pub fn new_with_attempt_timeout(
        max_attempts: u32,
        max_operation_elapsed: Option<Duration>,
        max_total_elapsed: Option<Duration>,
        delay: RetryDelay,
        jitter: RetryJitter,
        attempt_timeout: Option<AttemptTimeoutOption>,
    ) -> Result<Self, RetryConfigError> {
        let max_attempts = NonZeroU32::new(max_attempts).ok_or_else(|| {
            RetryConfigError::invalid_value(
                KEY_MAX_ATTEMPTS,
                "max_attempts must be greater than zero",
            )
        })?;
        let options = Self {
            max_attempts,
            max_operation_elapsed,
            max_total_elapsed,
            delay,
            jitter,
            attempt_timeout,
            worker_cancel_grace: Duration::from_millis(DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS),
        };
        options.validate()?;
        Ok(options)
    }

    /// Reads a retry option snapshot from a `ConfigReader`.
    ///
    /// Keys are relative to the reader. Use `config.prefix_view("retry")` when
    /// the retry settings are nested under a `retry.` prefix.
    ///
    /// # Parameters
    /// - `config`: Configuration reader whose keys are relative to the retry
    ///   configuration prefix.
    ///
    /// # Returns
    /// A validated [`RetryOptions`] value. Missing keys fall back to
    /// [`RetryOptions::default`].
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when a key cannot be read as the expected
    /// type, the delay strategy name is unsupported, or the resulting options
    /// fail validation.
    #[cfg(feature = "config")]
    pub fn from_config<R>(config: &R) -> Result<Self, RetryConfigError>
    where
        R: ConfigReader + ?Sized,
    {
        let default = Self::default();
        let values = RetryConfigValues::new(config).map_err(RetryConfigError::from)?;
        values.to_options(&default)
    }

    /// Validates all options.
    ///
    /// # Returns
    /// `Ok(())` when all contained strategy parameters are usable.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] with the relevant config key when the delay
    /// or jitter strategy is invalid.
    pub fn validate(&self) -> Result<(), RetryConfigError> {
        self.delay
            .validate()
            .map_err(|message| RetryConfigError::invalid_value(KEY_DELAY, message))?;
        self.jitter
            .validate()
            .map_err(|message| RetryConfigError::invalid_value(KEY_JITTER_FACTOR, message))?;
        if let Some(attempt_timeout) = self.attempt_timeout {
            attempt_timeout.validate().map_err(|message| {
                RetryConfigError::invalid_value(KEY_ATTEMPT_TIMEOUT_MILLIS, message)
            })?;
        }
        Ok(())
    }

    /// Calculates the base retry delay for one failed-attempt index.
    ///
    /// # Parameters
    /// - `attempt`: Failed-attempt index, starting at 1.
    ///
    /// # Returns
    /// Base delay before jitter.
    pub fn base_delay_for_attempt(&self, attempt: u32) -> Duration {
        self.delay.base_delay(attempt)
    }

    /// Calculates the retry delay for one failed-attempt index after jitter.
    ///
    /// # Parameters
    /// - `attempt`: Failed-attempt index, starting at 1.
    ///
    /// # Returns
    /// Delay after jitter is applied.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        self.jitter.delay_for_attempt(&self.delay, attempt)
    }

    /// Calculates the next base delay from the current base delay.
    ///
    /// For exponential delay, this advances by one multiplier step from
    /// `current` and caps at `max`; `Duration::ZERO` represents no previous
    /// base delay and returns the exponential initial delay. For other
    /// strategies, this delegates to the strategy's per-attempt base behavior.
    ///
    /// # Parameters
    /// - `current`: Current base delay before jitter.
    ///
    /// # Returns
    /// Next base delay before jitter.
    pub fn next_base_delay_from_current(&self, current: Duration) -> Duration {
        match &self.delay {
            RetryDelay::None => Duration::ZERO,
            RetryDelay::Fixed(delay) => *delay,
            RetryDelay::Random { .. } => self.delay.base_delay(1),
            RetryDelay::Exponential {
                initial,
                max,
                multiplier,
            } => {
                if current.is_zero() {
                    return *initial;
                }
                let bounded_current = current.min(*max);
                let next = bounded_current.mul_f64(*multiplier);
                if next > *max { *max } else { next }
            }
        }
    }

    /// Applies configured jitter to `base_delay`.
    ///
    /// # Parameters
    /// - `base_delay`: Base delay before jitter.
    ///
    /// # Returns
    /// Delay after jitter.
    pub fn jittered_delay(&self, base_delay: Duration) -> Duration {
        self.jitter.apply(base_delay)
    }

    /// Calculates the next delay from the current base delay and applies jitter.
    ///
    /// # Parameters
    /// - `current`: Current base delay before jitter.
    ///
    /// # Returns
    /// Next delay after jitter.
    pub fn next_delay_from_current(&self, current: Duration) -> Duration {
        self.jittered_delay(self.next_base_delay_from_current(current))
    }

    /// Returns the configured attempt-timeout duration.
    ///
    /// # Returns
    /// `Some(Duration)` when per-attempt timeout is configured.
    #[inline]
    pub(crate) fn attempt_timeout_duration(&self) -> Option<Duration> {
        self.attempt_timeout
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
    pub(crate) fn effective_attempt_timeout(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> EffectiveAttemptTimeout {
        // Build all timeout candidates with their source, then pick the
        // shortest remaining duration. Keeping the source is important: a
        // timeout caused by an elapsed budget is terminal, while a configured
        // attempt timeout still follows AttemptTimeoutPolicy.
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
        // AttemptTimeoutSource derives Ord in the precedence we need for ties:
        // Configured wins over elapsed-budget candidates so a configured
        // timeout exactly equal to the remaining budget remains observable as a
        // configured attempt timeout.
        match selected {
            Some((duration, source)) => EffectiveAttemptTimeout::new(Some(duration), Some(source)),
            None => EffectiveAttemptTimeout::none(),
        }
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
    pub(crate) fn elapsed_error_reason(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> Option<RetryErrorReason> {
        if self
            .max_operation_elapsed
            .is_some_and(|max_operation_elapsed| operation_elapsed >= max_operation_elapsed)
        {
            Some(RetryErrorReason::MaxOperationElapsedExceeded)
        } else if self
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
    pub(crate) fn retry_sleep_exhausts_total_elapsed(
        &self,
        total_elapsed: Duration,
        delay: Duration,
    ) -> bool {
        if delay.is_zero() {
            return false;
        }
        let Some(max_total_elapsed) = self.max_total_elapsed else {
            return false;
        };
        total_elapsed.saturating_add(delay) >= max_total_elapsed
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
    pub(crate) fn retry_delay(
        &self,
        decision: AttemptFailureDecision,
        attempts: u32,
        hint: Option<Duration>,
    ) -> Duration {
        // Delay precedence is part of the public retry contract:
        // RetryAfter is an explicit listener override, retry-after hints apply
        // only when listeners left the default policy in charge, and Retry uses
        // the configured strategy directly.
        match decision {
            AttemptFailureDecision::RetryAfter(delay) => delay,
            AttemptFailureDecision::UseDefault => {
                hint.unwrap_or_else(|| self.jitter.delay_for_attempt(&self.delay, attempts))
            }
            AttemptFailureDecision::Retry | AttemptFailureDecision::Abort => {
                self.jitter.delay_for_attempt(&self.delay, attempts)
            }
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
        self.max_operation_elapsed
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
        self.max_total_elapsed
            .map(|max_total_elapsed| max_total_elapsed.saturating_sub(total_elapsed))
    }
}

impl Default for RetryOptions {
    /// Creates the default retry options.
    ///
    /// # Returns
    /// Options with five attempts, no cumulative user operation time limit,
    /// exponential delay, no jitter, and the default worker cancellation grace.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic because the hard-coded attempt count is
    /// non-zero.
    #[inline]
    fn default() -> Self {
        Self {
            max_attempts: NonZeroU32::new(DEFAULT_RETRY_MAX_ATTEMPTS)
                .expect("default retry attempts must be non-zero"),
            max_operation_elapsed: DEFAULT_RETRY_MAX_OPERATION_ELAPSED,
            max_total_elapsed: DEFAULT_RETRY_MAX_TOTAL_ELAPSED,
            delay: RetryDelay::default(),
            jitter: RetryJitter::default(),
            attempt_timeout: None,
            worker_cancel_grace: Duration::from_millis(DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS),
        }
    }
}
