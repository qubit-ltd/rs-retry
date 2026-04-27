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

use std::str::FromStr;
use std::time::Duration;

use qubit_config::{ConfigReader, ConfigResult};

use super::attempt_timeout_option::AttemptTimeoutOption;
use super::attempt_timeout_policy::AttemptTimeoutPolicy;
use super::retry_delay::RetryDelay;
use super::retry_jitter::RetryJitter;
use super::retry_options::RetryOptions;

use crate::RetryConfigError;
use crate::constants::{
    DEFAULT_RETRY_EXPONENTIAL_INITIAL_DELAY_MILLIS, DEFAULT_RETRY_EXPONENTIAL_MAX_DELAY_MILLIS,
    DEFAULT_RETRY_EXPONENTIAL_MULTIPLIER, DEFAULT_RETRY_JITTER_FACTOR,
    DEFAULT_RETRY_RANDOM_MAX_DELAY_MILLIS, DEFAULT_RETRY_RANDOM_MIN_DELAY_MILLIS,
    KEY_ATTEMPT_TIMEOUT_MILLIS, KEY_ATTEMPT_TIMEOUT_POLICY, KEY_DELAY, KEY_DELAY_STRATEGY,
    KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS, KEY_EXPONENTIAL_MAX_DELAY_MILLIS,
    KEY_EXPONENTIAL_MULTIPLIER, KEY_FIXED_DELAY_MILLIS, KEY_JITTER_FACTOR, KEY_MAX_ATTEMPTS,
    KEY_MAX_ELAPSED_MILLIS, KEY_MAX_ELAPSED_UNLIMITED, KEY_RANDOM_MAX_DELAY_MILLIS,
    KEY_RANDOM_MIN_DELAY_MILLIS,
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
    /// Optional cumulative user operation elapsed-time budget in milliseconds.
    pub max_elapsed_millis: Option<u64>,
    /// Optional explicit switch for unlimited user operation elapsed-time budget.
    pub max_elapsed_unlimited: Option<bool>,
    /// Optional attempt timeout in milliseconds.
    pub attempt_timeout_millis: Option<u64>,
    /// Optional action selected when one attempt times out.
    pub attempt_timeout_policy: Option<String>,
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
            attempt_timeout_millis: config.get_optional(KEY_ATTEMPT_TIMEOUT_MILLIS)?,
            attempt_timeout_policy: config.get_optional_string(KEY_ATTEMPT_TIMEOUT_POLICY)?,
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
        let attempt_timeout = self.get_attempt_timeout(default)?;
        let delay = self.get_delay(default)?;
        let jitter = self.get_jitter(default);
        RetryOptions::new_with_attempt_timeout(
            max_attempts,
            max_elapsed,
            delay,
            jitter,
            attempt_timeout,
        )
    }

    /// Resolves the cumulative user operation elapsed-time budget.
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

    /// Resolves per-attempt timeout settings.
    ///
    /// # Parameters
    /// - `default`: Default options used when timeout keys are absent.
    ///
    /// # Returns
    /// `Ok(Some(AttemptTimeoutOption))` when a timeout is configured, or
    /// `Ok(None)` when per-attempt timeout is disabled.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when policy text is unsupported or when a
    /// policy is configured without a timeout and no default timeout exists.
    fn get_attempt_timeout(
        &self,
        default: &RetryOptions,
    ) -> Result<Option<AttemptTimeoutOption>, RetryConfigError> {
        let default_attempt_timeout = default.attempt_timeout();
        let policy = self
            .attempt_timeout_policy
            .as_deref()
            .map(parse_attempt_timeout_policy)
            .transpose()?;

        match self.attempt_timeout_millis {
            Some(timeout_millis) => {
                let policy = policy
                    .or_else(|| {
                        default_attempt_timeout.map(|attempt_timeout| attempt_timeout.policy())
                    })
                    .unwrap_or_default();
                Ok(Some(AttemptTimeoutOption::new(
                    Duration::from_millis(timeout_millis),
                    policy,
                )))
            }
            None => {
                if let Some(policy) = policy {
                    let Some(default_attempt_timeout) = default_attempt_timeout else {
                        return Err(RetryConfigError::invalid_value(
                            KEY_ATTEMPT_TIMEOUT_POLICY,
                            "attempt_timeout_policy requires attempt_timeout_millis when the default has no attempt timeout",
                        ));
                    };
                    Ok(Some(default_attempt_timeout.with_policy(policy)))
                } else {
                    Ok(default_attempt_timeout)
                }
            }
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
            .map(|value| (KEY_DELAY, value))
            .or_else(|| {
                self.delay_strategy
                    .as_deref()
                    .map(|value| (KEY_DELAY_STRATEGY, value))
            })
            .map(|(key, value)| (key, value.trim().to_ascii_lowercase()));
        match strategy {
            None => Ok(self
                .get_implicit_delay()
                .unwrap_or_else(|| default.delay().clone())),
            Some((_, strategy)) if strategy == "none" => Ok(RetryDelay::None),
            Some((_, strategy)) if strategy == "fixed" => {
                let Some(fixed_delay_millis) = self.fixed_delay_millis else {
                    return Err(RetryConfigError::invalid_value(
                        KEY_FIXED_DELAY_MILLIS,
                        "fixed delay strategy requires fixed_delay_millis",
                    ));
                };
                Ok(RetryDelay::fixed(Duration::from_millis(fixed_delay_millis)))
            }
            Some((_, strategy)) if strategy == "random" => Ok(RetryDelay::random(
                Duration::from_millis(self.random_min_delay_millis.ok_or_else(|| {
                    RetryConfigError::invalid_value(
                        KEY_RANDOM_MIN_DELAY_MILLIS,
                        "random delay strategy requires random_min_delay_millis",
                    )
                })?),
                Duration::from_millis(self.random_max_delay_millis.ok_or_else(|| {
                    RetryConfigError::invalid_value(
                        KEY_RANDOM_MAX_DELAY_MILLIS,
                        "random delay strategy requires random_max_delay_millis",
                    )
                })?),
            )),
            Some((_, strategy))
                if strategy == "exponential" || strategy == "exponential_backoff" =>
            {
                let initial_delay = self.exponential_initial_delay_millis.ok_or_else(|| {
                    RetryConfigError::invalid_value(
                        KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS,
                        "exponential delay strategy requires exponential_initial_delay_millis",
                    )
                })?;
                let max_delay = self.exponential_max_delay_millis.ok_or_else(|| {
                    RetryConfigError::invalid_value(
                        KEY_EXPONENTIAL_MAX_DELAY_MILLIS,
                        "exponential delay strategy requires exponential_max_delay_millis",
                    )
                })?;
                let multiplier = self.exponential_multiplier.ok_or_else(|| {
                    RetryConfigError::invalid_value(
                        KEY_EXPONENTIAL_MULTIPLIER,
                        "exponential delay strategy requires exponential_multiplier",
                    )
                })?;
                Ok(RetryDelay::exponential(
                    Duration::from_millis(initial_delay),
                    Duration::from_millis(max_delay),
                    multiplier,
                ))
            }
            Some((key, other)) => Err(RetryConfigError::invalid_value(
                key,
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
                Duration::from_millis(
                    self.random_min_delay_millis
                        .unwrap_or(DEFAULT_RETRY_RANDOM_MIN_DELAY_MILLIS),
                ),
                Duration::from_millis(
                    self.random_max_delay_millis
                        .unwrap_or(DEFAULT_RETRY_RANDOM_MAX_DELAY_MILLIS),
                ),
            ));
        }
        if self.exponential_initial_delay_millis.is_some()
            || self.exponential_max_delay_millis.is_some()
            || self.exponential_multiplier.is_some()
        {
            return Some(RetryDelay::exponential(
                Duration::from_millis(
                    self.exponential_initial_delay_millis
                        .unwrap_or(DEFAULT_RETRY_EXPONENTIAL_INITIAL_DELAY_MILLIS),
                ),
                Duration::from_millis(
                    self.exponential_max_delay_millis
                        .unwrap_or(DEFAULT_RETRY_EXPONENTIAL_MAX_DELAY_MILLIS),
                ),
                self.exponential_multiplier
                    .unwrap_or(DEFAULT_RETRY_EXPONENTIAL_MULTIPLIER),
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
            Some(factor) if factor == DEFAULT_RETRY_JITTER_FACTOR => RetryJitter::None,
            None => default.jitter(),
            Some(factor) => RetryJitter::Factor(factor),
        }
    }
}

/// Parses a configured attempt-timeout policy.
///
/// # Parameters
/// - `value`: Raw policy text read from configuration.
///
/// # Returns
/// A parsed [`AttemptTimeoutPolicy`].
///
/// # Errors
/// Returns [`RetryConfigError`] when the policy text is unsupported.
fn parse_attempt_timeout_policy(value: &str) -> Result<AttemptTimeoutPolicy, RetryConfigError> {
    AttemptTimeoutPolicy::from_str(value)
        .map_err(|message| RetryConfigError::invalid_value(KEY_ATTEMPT_TIMEOUT_POLICY, message))
}
