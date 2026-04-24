/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Raw retry configuration values from `qubit-config` and merge into
//! [`RetryOptions`](crate::options::RetryOptions).
//!
//! Author: Haixing Hu

use std::time::Duration;

use qubit_config::{ConfigReader, ConfigResult};

use super::retry_delay::RetryDelay;
use super::retry_jitter::RetryJitter;
use super::retry_options::RetryOptions;

use crate::RetryConfigError;
use crate::constants::{
    KEY_DELAY, KEY_DELAY_STRATEGY, KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS,
    KEY_EXPONENTIAL_MAX_DELAY_MILLIS, KEY_EXPONENTIAL_MULTIPLIER, KEY_FIXED_DELAY_MILLIS,
    KEY_JITTER_FACTOR, KEY_MAX_ATTEMPTS, KEY_MAX_ELAPSED_MILLIS, KEY_MAX_ELAPSED_UNLIMITED,
    KEY_RANDOM_MAX_DELAY_MILLIS, KEY_RANDOM_MIN_DELAY_MILLIS,
};

/// Raw retry configuration values read from `qubit-config`.
///
/// This struct deliberately keeps all `ConfigReader` calls in one place. The
/// conversion from `qubit-config` errors to retry-specific errors happens at
/// the caller boundary, while the remaining methods only translate already
/// typed values into retry domain objects.
///
/// Fields are public so callers and integration tests can build snapshots
/// programmatically and merge them with [`RetryConfigValues::to_options`].
///
/// Author: Haixing Hu
#[derive(Debug, Clone, PartialEq)]
pub struct RetryConfigValues {
    /// Optional maximum attempts value.
    pub max_attempts: Option<u32>,
    /// Optional elapsed-time budget in milliseconds.
    pub max_elapsed_millis: Option<u64>,
    /// Optional explicit switch for unlimited elapsed-time budget.
    pub max_elapsed_unlimited: Option<bool>,
    /// Optional primary delay strategy name.
    pub delay: Option<String>,
    /// Optional backward-compatible delay strategy alias.
    pub delay_strategy: Option<String>,
    /// Optional fixed delay in milliseconds.
    pub fixed_delay_millis: Option<u64>,
    /// Optional random delay lower bound in milliseconds.
    pub random_min_delay_millis: Option<u64>,
    /// Optional random delay upper bound in milliseconds.
    pub random_max_delay_millis: Option<u64>,
    /// Optional exponential initial delay in milliseconds.
    pub exponential_initial_delay_millis: Option<u64>,
    /// Optional exponential maximum delay in milliseconds.
    pub exponential_max_delay_millis: Option<u64>,
    /// Optional exponential multiplier.
    pub exponential_multiplier: Option<f64>,
    /// Optional jitter factor.
    pub jitter_factor: Option<f64>,
}

impl RetryConfigValues {
    /// Creates a snapshot by reading all retry-related configuration values.
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
    pub(crate) fn new<R>(config: &R) -> ConfigResult<Self>
    where
        R: ConfigReader + ?Sized,
    {
        Ok(Self {
            max_attempts: config.get_optional(KEY_MAX_ATTEMPTS)?,
            max_elapsed_millis: config.get_optional(KEY_MAX_ELAPSED_MILLIS)?,
            max_elapsed_unlimited: config.get_optional(KEY_MAX_ELAPSED_UNLIMITED)?,
            delay: config.get_optional_string(KEY_DELAY)?,
            delay_strategy: config.get_optional_string(KEY_DELAY_STRATEGY)?,
            fixed_delay_millis: config.get_optional(KEY_FIXED_DELAY_MILLIS)?,
            random_min_delay_millis: config.get_optional(KEY_RANDOM_MIN_DELAY_MILLIS)?,
            random_max_delay_millis: config.get_optional(KEY_RANDOM_MAX_DELAY_MILLIS)?,
            exponential_initial_delay_millis: config
                .get_optional(KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS)?,
            exponential_max_delay_millis: config.get_optional(KEY_EXPONENTIAL_MAX_DELAY_MILLIS)?,
            exponential_multiplier: config.get_optional(KEY_EXPONENTIAL_MULTIPLIER)?,
            jitter_factor: config.get_optional(KEY_JITTER_FACTOR)?,
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
    pub fn to_options(&self, default: &RetryOptions) -> Result<RetryOptions, RetryConfigError> {
        let max_attempts = self.max_attempts.unwrap_or(default.max_attempts());
        let max_elapsed = self.get_max_elapsed(default);
        let delay = self.get_delay(default)?;
        let jitter = self.get_jitter(default);
        RetryOptions::new(max_attempts, max_elapsed, delay, jitter)
    }

    /// Resolves the elapsed-time budget.
    ///
    /// # Parameters
    /// - `default`: Fallback when `max_elapsed_millis` is absent from config.
    ///
    /// # Returns
    /// - `None` when `max_elapsed_unlimited` is configured as `true`.
    /// - `Some(Duration)` when `max_elapsed_millis` is present (including zero).
    /// - `default.max_elapsed` when the key is absent.
    ///
    /// # Errors
    /// This method does not return errors.
    fn get_max_elapsed(&self, default: &RetryOptions) -> Option<Duration> {
        if self.max_elapsed_unlimited.unwrap_or(false) {
            return None;
        }
        match self.max_elapsed_millis {
            Some(millis) => Some(Duration::from_millis(millis)),
            None => default.max_elapsed(),
        }
    }

    /// Resolves the base delay strategy.
    ///
    /// # Parameters
    /// - `default`: Default options used when neither explicit nor implicit
    ///   delay configuration is present.
    ///
    /// # Returns
    /// The explicit, implicit, or default [`RetryDelay`] strategy.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when the explicit delay strategy name is
    /// unsupported.
    fn get_delay(&self, default: &RetryOptions) -> Result<RetryDelay, RetryConfigError> {
        let strategy = self
            .delay
            .as_deref()
            .or(self.delay_strategy.as_deref())
            .map(str::trim)
            .map(|value| value.to_ascii_lowercase());
        match strategy.as_deref() {
            None => Ok(self
                .get_implicit_delay()
                .unwrap_or_else(|| default.delay().clone())),
            Some("none") => Ok(RetryDelay::None),
            Some("fixed") => Ok(RetryDelay::fixed(Duration::from_millis(
                self.fixed_delay_millis.unwrap_or(1000),
            ))),
            Some("random") => Ok(RetryDelay::random(
                Duration::from_millis(self.random_min_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.random_max_delay_millis.unwrap_or(10000)),
            )),
            Some("exponential") | Some("exponential_backoff") => Ok(RetryDelay::exponential(
                Duration::from_millis(self.exponential_initial_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.exponential_max_delay_millis.unwrap_or(60000)),
                self.exponential_multiplier.unwrap_or(2.0),
            )),
            Some(other) => Err(RetryConfigError::invalid_value(
                KEY_DELAY,
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
    /// `Some(RetryDelay)` when any delay parameter key is present; otherwise `None`.
    ///
    /// # Errors
    /// This method does not return errors because all config reads have already
    /// succeeded.
    fn get_implicit_delay(&self) -> Option<RetryDelay> {
        if let Some(millis) = self.fixed_delay_millis {
            return Some(RetryDelay::fixed(Duration::from_millis(millis)));
        }
        if self.random_min_delay_millis.is_some() || self.random_max_delay_millis.is_some() {
            return Some(RetryDelay::random(
                Duration::from_millis(self.random_min_delay_millis.unwrap_or(1000)),
                Duration::from_millis(self.random_max_delay_millis.unwrap_or(10000)),
            ));
        }
        if self.exponential_initial_delay_millis.is_some()
            || self.exponential_max_delay_millis.is_some()
            || self.exponential_multiplier.is_some()
        {
            return Some(RetryDelay::exponential(
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
    ///   jitter factor key is absent.
    ///
    /// # Returns
    /// The configured or default [`RetryJitter`] strategy.
    ///
    /// # Errors
    /// This method does not return errors. RetryJitter value validation is handled
    /// by [`RetryOptions::new`].
    fn get_jitter(&self, default: &RetryOptions) -> RetryJitter {
        match self.jitter_factor {
            Some(0.0) => RetryJitter::None,
            None => default.jitter(),
            Some(factor) => RetryJitter::Factor(factor),
        }
    }
}
