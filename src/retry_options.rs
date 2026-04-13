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
//! This module contains the immutable options consumed by [`crate::RetryExecutor`]
//! and the private helpers that translate `qubit-config` values into strongly
//! typed retry settings.

use std::num::NonZeroU32;
use std::time::Duration;

use qubit_config::{ConfigReader, ConfigResult};

use crate::{Delay, Jitter, RetryConfigError};

/// Immutable retry option snapshot used by [`crate::RetryExecutor`].
///
/// `RetryOptions` owns all executor configuration that is independent of the
/// application error type: attempt limits, total elapsed-time budget, delay
/// strategy, and jitter strategy. Construction validates the delay and jitter
/// values before an executor can use them.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryOptions {
    /// Maximum attempts, including the initial attempt.
    pub max_attempts: NonZeroU32,
    /// Maximum total elapsed time for the retry flow, in milliseconds.
    pub max_elapsed: Option<Duration>,
    /// Base delay strategy between attempts.
    pub delay: Delay,
    /// Jitter applied to each base delay.
    pub jitter: Jitter,
}

impl RetryOptions {
    /// Key for maximum attempts.
    pub const KEY_MAX_ATTEMPTS: &'static str = "max_attempts";
    /// Key for maximum elapsed budget in milliseconds. Missing means unlimited;
    /// zero also maps to unlimited when read from config.
    pub const KEY_MAX_ELAPSED_MILLIS: &'static str = "max_elapsed_millis";
    /// Key for delay strategy name.
    pub const KEY_DELAY: &'static str = "delay";
    /// Backward-compatible alias for delay strategy name.
    pub const KEY_DELAY_STRATEGY: &'static str = "delay_strategy";
    /// Key for fixed delay in milliseconds.
    pub const KEY_FIXED_DELAY_MILLIS: &'static str = "fixed_delay_millis";
    /// Key for random minimum delay in milliseconds.
    pub const KEY_RANDOM_MIN_DELAY_MILLIS: &'static str = "random_min_delay_millis";
    /// Key for random maximum delay in milliseconds.
    pub const KEY_RANDOM_MAX_DELAY_MILLIS: &'static str = "random_max_delay_millis";
    /// Key for exponential initial delay in milliseconds.
    pub const KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS: &'static str =
        "exponential_initial_delay_millis";
    /// Key for exponential maximum delay in milliseconds.
    pub const KEY_EXPONENTIAL_MAX_DELAY_MILLIS: &'static str = "exponential_max_delay_millis";
    /// Key for exponential multiplier.
    pub const KEY_EXPONENTIAL_MULTIPLIER: &'static str = "exponential_multiplier";
    /// Key for jitter factor.
    pub const KEY_JITTER_FACTOR: &'static str = "jitter_factor";

    /// Creates and validates a retry option snapshot.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum number of attempts, including the first call.
    ///   Must be greater than zero.
    /// - `max_elapsed`: Optional total elapsed-time budget for all attempts
    ///   and sleeps.
    /// - `delay`: Base delay strategy used between attempts.
    /// - `jitter`: Jitter strategy applied to each base delay.
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
        delay: Delay,
        jitter: Jitter,
    ) -> Result<Self, RetryConfigError> {
        let max_attempts = NonZeroU32::new(max_attempts).ok_or_else(|| {
            RetryConfigError::invalid_value(
                Self::KEY_MAX_ATTEMPTS,
                "max_attempts must be greater than zero",
            )
        })?;
        let options = Self {
            max_attempts,
            max_elapsed,
            delay,
            jitter,
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
    pub fn from_config<R>(config: &R) -> Result<Self, RetryConfigError>
    where
        R: ConfigReader + ?Sized,
    {
        let default = Self::default();
        let values = RetryConfigValues::read_from(config).map_err(RetryConfigError::from)?;
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
            .map_err(|message| RetryConfigError::invalid_value(Self::KEY_DELAY, message))?;
        self.jitter
            .validate()
            .map_err(|message| RetryConfigError::invalid_value(Self::KEY_JITTER_FACTOR, message))?;
        Ok(())
    }
}

impl Default for RetryOptions {
    /// Creates the default retry options.
    ///
    /// # Returns
    /// Options with three attempts, no total elapsed-time limit, exponential
    /// delay, and no jitter.
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
            max_attempts: NonZeroU32::new(3).expect("default retry attempts must be non-zero"),
            max_elapsed: None,
            delay: Delay::default(),
            jitter: Jitter::None,
        }
    }
}

/// Raw retry configuration values read from `qubit-config`.
///
/// This struct deliberately keeps all `ConfigReader` calls in one place. The
/// conversion from `qubit-config` errors to retry-specific errors happens at
/// the caller boundary, while the remaining methods only translate already
/// typed values into retry domain objects.
#[derive(Debug, Clone, PartialEq)]
struct RetryConfigValues {
    /// Optional maximum attempts value.
    max_attempts: Option<u32>,
    /// Optional elapsed-time budget in milliseconds.
    max_elapsed_millis: Option<u64>,
    /// Optional primary delay strategy name.
    delay: Option<String>,
    /// Optional backward-compatible delay strategy alias.
    delay_strategy: Option<String>,
    /// Optional fixed delay in milliseconds.
    fixed_delay_millis: Option<u64>,
    /// Optional random delay lower bound in milliseconds.
    random_min_delay_millis: Option<u64>,
    /// Optional random delay upper bound in milliseconds.
    random_max_delay_millis: Option<u64>,
    /// Optional exponential initial delay in milliseconds.
    exponential_initial_delay_millis: Option<u64>,
    /// Optional exponential maximum delay in milliseconds.
    exponential_max_delay_millis: Option<u64>,
    /// Optional exponential multiplier.
    exponential_multiplier: Option<f64>,
    /// Optional jitter factor.
    jitter_factor: Option<f64>,
}

impl RetryConfigValues {
    /// Reads all retry-related configuration values.
    ///
    /// # Parameters
    /// - `config`: Configuration reader whose keys are relative to the retry
    ///   configuration prefix.
    ///
    /// # Returns
    /// A [`RetryConfigValues`] snapshot containing every retry option key
    /// understood by this crate.
    ///
    /// # Errors
    /// Returns `qubit-config`'s `ConfigError` through [`ConfigResult`] when any
    /// present key cannot be read as the expected type or string substitution
    /// fails.
    fn read_from<R>(config: &R) -> ConfigResult<Self>
    where
        R: ConfigReader + ?Sized,
    {
        Ok(Self {
            max_attempts: config.get_optional(RetryOptions::KEY_MAX_ATTEMPTS)?,
            max_elapsed_millis: config.get_optional(RetryOptions::KEY_MAX_ELAPSED_MILLIS)?,
            delay: config.get_optional_string(RetryOptions::KEY_DELAY)?,
            delay_strategy: config.get_optional_string(RetryOptions::KEY_DELAY_STRATEGY)?,
            fixed_delay_millis: config.get_optional(RetryOptions::KEY_FIXED_DELAY_MILLIS)?,
            random_min_delay_millis: config
                .get_optional(RetryOptions::KEY_RANDOM_MIN_DELAY_MILLIS)?,
            random_max_delay_millis: config
                .get_optional(RetryOptions::KEY_RANDOM_MAX_DELAY_MILLIS)?,
            exponential_initial_delay_millis: config
                .get_optional(RetryOptions::KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS)?,
            exponential_max_delay_millis: config
                .get_optional(RetryOptions::KEY_EXPONENTIAL_MAX_DELAY_MILLIS)?,
            exponential_multiplier: config
                .get_optional(RetryOptions::KEY_EXPONENTIAL_MULTIPLIER)?,
            jitter_factor: config.get_optional(RetryOptions::KEY_JITTER_FACTOR)?,
        })
    }

    /// Converts the raw configuration snapshot into validated retry options.
    ///
    /// # Parameters
    /// - `default`: Default options used when a config key is absent.
    ///
    /// # Returns
    /// A validated [`RetryOptions`] value.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when the delay strategy name is unsupported
    /// or the resulting options fail validation.
    fn to_options(&self, default: &RetryOptions) -> Result<RetryOptions, RetryConfigError> {
        let max_attempts = self.max_attempts.unwrap_or(default.max_attempts.get());
        let max_elapsed = self.max_elapsed();
        let delay = self.delay(default)?;
        let jitter = self.jitter(default);
        RetryOptions::new(max_attempts, max_elapsed, delay, jitter)
    }

    /// Resolves the elapsed-time budget.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(Duration)` when `max_elapsed_millis` is present and non-zero;
    /// otherwise `None`.
    ///
    /// # Errors
    /// This method does not return errors.
    fn max_elapsed(&self) -> Option<Duration> {
        match self.max_elapsed_millis {
            Some(0) | None => None,
            Some(millis) => Some(Duration::from_millis(millis)),
        }
    }

    /// Resolves the base delay strategy.
    ///
    /// # Parameters
    /// - `default`: Default options used when neither explicit nor implicit
    ///   delay configuration is present.
    ///
    /// # Returns
    /// The explicit, implicit, or default [`Delay`] strategy.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when the explicit delay strategy name is
    /// unsupported.
    fn delay(&self, default: &RetryOptions) -> Result<Delay, RetryConfigError> {
        let strategy = self
            .delay
            .as_deref()
            .or(self.delay_strategy.as_deref())
            .map(str::trim)
            .map(|value| value.to_ascii_lowercase());
        match strategy.as_deref() {
            None => Ok(self
                .implicit_delay()
                .unwrap_or_else(|| default.delay.clone())),
            Some("none") => Ok(Delay::None),
            Some("fixed") => Ok(Delay::fixed(Duration::from_millis(
                self.fixed_delay_millis.unwrap_or(1000),
            ))),
            Some("random") => Ok(Delay::random(
                Duration::from_millis(self.random_min_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.random_max_delay_millis.unwrap_or(10000)),
            )),
            Some("exponential") | Some("exponential_backoff") => Ok(Delay::exponential(
                Duration::from_millis(self.exponential_initial_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.exponential_max_delay_millis.unwrap_or(60000)),
                self.exponential_multiplier.unwrap_or(2.0),
            )),
            Some(other) => Err(RetryConfigError::invalid_value(
                RetryOptions::KEY_DELAY,
                format!("unsupported delay strategy '{other}'"),
            )),
        }
    }

    /// Resolves a delay strategy from parameter keys when no strategy name is configured.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(Delay)` when any delay parameter key is present; otherwise `None`.
    ///
    /// # Errors
    /// This method does not return errors because all config reads have already
    /// succeeded.
    fn implicit_delay(&self) -> Option<Delay> {
        if let Some(millis) = self.fixed_delay_millis {
            return Some(Delay::fixed(Duration::from_millis(millis)));
        }
        if self.random_min_delay_millis.is_some() || self.random_max_delay_millis.is_some() {
            return Some(Delay::random(
                Duration::from_millis(self.random_min_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.random_max_delay_millis.unwrap_or(10000)),
            ));
        }
        if self.exponential_initial_delay_millis.is_some()
            || self.exponential_max_delay_millis.is_some()
            || self.exponential_multiplier.is_some()
        {
            return Some(Delay::exponential(
                Duration::from_millis(self.exponential_initial_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.exponential_max_delay_millis.unwrap_or(60000)),
                self.exponential_multiplier.unwrap_or(2.0),
            ));
        }
        None
    }

    /// Resolves the jitter strategy.
    ///
    /// # Parameters
    /// - `default`: Default options used when no jitter key is present or the
    ///   configured jitter factor is `0.0`.
    ///
    /// # Returns
    /// The configured or default [`Jitter`] strategy.
    ///
    /// # Errors
    /// This method does not return errors. Jitter value validation is handled
    /// by [`RetryOptions::new`].
    fn jitter(&self, default: &RetryOptions) -> Jitter {
        match self.jitter_factor {
            Some(0.0) | None => default.jitter,
            Some(factor) => Jitter::Factor(factor),
        }
    }
}
