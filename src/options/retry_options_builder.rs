// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Builder for the legacy configuration snapshot.

use std::num::NonZeroU32;
use std::time::Duration;

use super::AttemptTimeoutOption;
use super::AttemptTimeoutPolicy;
use super::RetryAfterPolicy;
use super::RetryDelay;
use super::RetryJitter;
use super::RetryOptions;
use crate::RetryConfigError;

/// Builds a validated [`RetryOptions`] snapshot.
#[must_use]
pub struct RetryOptionsBuilder {
    options: RetryOptions,
    max_attempts_error: Option<RetryConfigError>,
}

impl RetryOptionsBuilder {
    /// Creates a builder with default values.
    pub fn new() -> Self {
        Self {
            options: RetryOptions::default(),
            max_attempts_error: None,
        }
    }

    /// Starts from an existing validated snapshot.
    pub fn options(mut self, options: RetryOptions) -> Self {
        self.options = options;
        self.max_attempts_error = None;
        self
    }

    /// Sets the maximum attempts, including the initial attempt.
    pub fn max_attempts(mut self, value: u32) -> Self {
        match NonZeroU32::new(value) {
            Some(value) => {
                self.options.max_attempts = value;
                self.max_attempts_error = None;
            }
            None => {
                self.max_attempts_error =
                    Some(RetryConfigError::invalid_value(
                        "max_attempts",
                        "max_attempts must be greater than zero",
                    ));
            }
        }
        self
    }

    /// Sets the maximum retries after the first attempt.
    pub fn max_retries(self, value: u32) -> Self {
        self.max_attempts(value.saturating_add(1))
    }

    /// Sets the cumulative operation budget.
    pub fn max_operation_elapsed(mut self, value: Option<Duration>) -> Self {
        self.options.max_operation_elapsed = value;
        self
    }

    /// Sets the total flow budget.
    pub fn max_total_elapsed(mut self, value: Option<Duration>) -> Self {
        self.options.max_total_elapsed = value;
        self
    }

    /// Sets the delay strategy.
    pub fn delay(mut self, value: RetryDelay) -> Self {
        self.options.delay = value;
        self
    }

    /// Disables delay.
    pub fn no_delay(self) -> Self {
        self.delay(RetryDelay::none())
    }

    /// Sets a fixed delay.
    pub fn fixed_delay(self, value: Duration) -> Self {
        self.delay(RetryDelay::fixed(value))
    }

    /// Sets a random delay range.
    pub fn random_delay(self, min: Duration, max: Duration) -> Self {
        self.delay(RetryDelay::random(min, max))
    }

    /// Sets exponential backoff.
    pub fn exponential_backoff(self, initial: Duration, max: Duration) -> Self {
        self.exponential_backoff_with_multiplier(initial, max, 2.0)
    }

    /// Sets exponential backoff with a multiplier.
    pub fn exponential_backoff_with_multiplier(
        self,
        initial: Duration,
        max: Duration,
        multiplier: f64,
    ) -> Self {
        self.delay(RetryDelay::exponential(initial, max, multiplier))
    }

    /// Sets jitter.
    pub fn jitter(mut self, value: RetryJitter) -> Self {
        self.options.jitter = value;
        self
    }

    /// Sets proportional jitter.
    pub fn jitter_factor(self, value: f64) -> Self {
        self.jitter(RetryJitter::factor(value))
    }

    /// Sets an attempt timeout.
    pub fn attempt_timeout(mut self, value: Option<Duration>) -> Self {
        self.options.attempt_timeout = value.map(|timeout| {
            AttemptTimeoutOption::new(
                timeout,
                self.options
                    .attempt_timeout
                    .map_or(AttemptTimeoutPolicy::Retry, |option| {
                        option.policy()
                    }),
            )
        });
        self
    }

    /// Sets a complete attempt timeout option.
    pub fn attempt_timeout_option(
        mut self,
        value: Option<AttemptTimeoutOption>,
    ) -> Self {
        self.options.attempt_timeout = value;
        self
    }

    /// Sets the attempt timeout policy.
    pub fn attempt_timeout_policy(
        mut self,
        value: AttemptTimeoutPolicy,
    ) -> Self {
        self.options.attempt_timeout = self
            .options
            .attempt_timeout
            .map(|option| option.with_policy(value));
        self
    }

    /// Retries configured attempt timeouts.
    pub fn retry_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Retry)
    }

    /// Aborts on configured attempt timeouts.
    pub fn abort_on_timeout(self) -> Self {
        self.attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
    }

    /// Sets worker cancellation grace.
    pub fn worker_cancel_grace(mut self, value: Duration) -> Self {
        self.options.worker_cancel_grace = value;
        self
    }

    /// Sets retry-after handling.
    pub fn retry_after_policy(mut self, value: RetryAfterPolicy) -> Self {
        self.options.retry_after_policy = value;
        self
    }

    /// Validates and returns the snapshot.
    pub fn build(self) -> Result<RetryOptions, RetryConfigError> {
        if let Some(error) = self.max_attempts_error {
            return Err(error);
        }
        self.options.validate()?;
        Ok(self.options)
    }
}

impl Default for RetryOptionsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
