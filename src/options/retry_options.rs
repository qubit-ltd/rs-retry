/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry option snapshot and configuration loading helpers.
//!
//! This module contains the immutable options consumed by [`crate::Retry`].
//! Raw config merge logic lives in [`crate::options::retry_config_values`].
//!
//! Author: Haixing Hu

use std::num::NonZeroU32;
use std::time::Duration;

#[cfg(feature = "config")]
use qubit_config::ConfigReader;

use super::attempt_timeout_option::AttemptTimeoutOption;
#[cfg(feature = "config")]
use super::retry_config_values::RetryConfigValues;

use crate::constants::{
    DEFAULT_RETRY_MAX_ATTEMPTS, DEFAULT_RETRY_MAX_ELAPSED,
    DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS, KEY_ATTEMPT_TIMEOUT_MILLIS, KEY_DELAY,
    KEY_JITTER_FACTOR, KEY_MAX_ATTEMPTS,
};
use crate::{RetryConfigError, RetryDelay, RetryJitter};

/// Immutable retry option snapshot used by [`crate::Retry`].
///
/// `RetryOptions` owns all executor configuration that is independent of the
/// application error type: attempt limits, cumulative user operation elapsed
/// budget, delay strategy, and jitter strategy. Construction validates the delay
/// and jitter values before an executor can use them.
///
/// Author: Haixing Hu
#[derive(Debug, Clone, PartialEq)]
pub struct RetryOptions {
    /// Maximum attempts, including the initial attempt.
    pub(crate) max_attempts: NonZeroU32,
    /// Maximum cumulative user operation time for the retry flow.
    pub(crate) max_elapsed: Option<Duration>,
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
    pub fn max_elapsed(&self) -> Option<Duration> {
        self.max_elapsed
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
    /// - `max_elapsed`: Optional cumulative user operation time budget for all
    ///   attempts. Listener execution and retry sleeps are excluded.
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
        max_elapsed: Option<Duration>,
        delay: RetryDelay,
        jitter: RetryJitter,
    ) -> Result<Self, RetryConfigError> {
        Self::new_with_attempt_timeout(max_attempts, max_elapsed, delay, jitter, None)
    }

    /// Creates and validates a retry option snapshot with attempt timeout.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum number of attempts, including the first call.
    ///   Must be greater than zero.
    /// - `max_elapsed`: Optional cumulative user operation time budget for all
    ///   attempts. Listener execution and retry sleeps are excluded.
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
        max_elapsed: Option<Duration>,
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
            max_elapsed,
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
    /// `current` and caps at `max`. For other strategies, this delegates to the
    /// strategy's per-attempt base behavior.
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
                max, multiplier, ..
            } => {
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
            max_elapsed: DEFAULT_RETRY_MAX_ELAPSED,
            delay: RetryDelay::default(),
            jitter: RetryJitter::default(),
            attempt_timeout: None,
            worker_cancel_grace: Duration::from_millis(DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS),
        }
    }
}
