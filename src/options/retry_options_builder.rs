// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder for error-type-independent retry options.

use std::time::Duration;

use qubit_error::BoxError;

use crate::{
    AttemptTimeoutOption,
    AttemptTimeoutPolicy,
    RetryAfterPolicy,
    RetryBuilder,
    RetryConfigError,
    RetryDelay,
    RetryJitter,
    RetryOptions,
};

/// Builds a validated [`RetryOptions`] snapshot without selecting an error
/// type.
#[must_use]
pub struct RetryOptionsBuilder {
    inner: RetryBuilder<BoxError>,
}

impl RetryOptionsBuilder {
    /// Creates a builder with default retry options.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: RetryBuilder::new(),
        }
    }
    /// Sets the maximum number of attempts, including the initial attempt.
    pub fn max_attempts(mut self, value: u32) -> Self {
        self.inner = self.inner.max_attempts(value);
        self
    }
    /// Sets the maximum retry count after the initial attempt.
    pub fn max_retries(mut self, value: u32) -> Self {
        self.inner = self.inner.max_retries(value);
        self
    }
    /// Sets the cumulative operation-time budget, or removes it with `None`.
    pub fn max_operation_elapsed(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.max_operation_elapsed(value);
        self
    }
    /// Sets the total retry-flow time budget, or removes it with `None`.
    pub fn max_total_elapsed(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.max_total_elapsed(value);
        self
    }
    /// Sets the delay strategy used between attempts.
    pub fn delay(mut self, value: RetryDelay) -> Self {
        self.inner = self.inner.delay(value);
        self
    }
    /// Disables delay between retry attempts.
    pub fn no_delay(mut self) -> Self {
        self.inner = self.inner.no_delay();
        self
    }
    /// Sets a fixed delay between retry attempts.
    pub fn fixed_delay(mut self, value: Duration) -> Self {
        self.inner = self.inner.fixed_delay(value);
        self
    }
    /// Sets a uniformly random delay range between retry attempts.
    pub fn random_delay(mut self, min: Duration, max: Duration) -> Self {
        self.inner = self.inner.random_delay(min, max);
        self
    }
    /// Sets exponential backoff with the default multiplier.
    pub fn exponential_backoff(
        mut self,
        initial: Duration,
        max: Duration,
    ) -> Self {
        self.inner = self.inner.exponential_backoff(initial, max);
        self
    }
    /// Sets exponential backoff with an explicit multiplier.
    pub fn exponential_backoff_with_multiplier(
        mut self,
        initial: Duration,
        max: Duration,
        multiplier: f64,
    ) -> Self {
        self.inner = self
            .inner
            .exponential_backoff_with_multiplier(initial, max, multiplier);
        self
    }
    /// Sets the jitter strategy applied to retry delays.
    pub fn jitter(mut self, value: RetryJitter) -> Self {
        self.inner = self.inner.jitter(value);
        self
    }
    /// Sets proportional jitter around each base delay.
    pub fn jitter_factor(mut self, value: f64) -> Self {
        self.inner = self.inner.jitter_factor(value);
        self
    }
    /// Sets the per-attempt timeout while retaining the pending timeout policy.
    pub fn attempt_timeout(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.attempt_timeout(value);
        self
    }
    /// Sets the complete per-attempt timeout option.
    pub fn attempt_timeout_option(
        mut self,
        value: Option<AttemptTimeoutOption>,
    ) -> Self {
        self.inner = self.inner.attempt_timeout_option(value);
        self
    }
    /// Sets the action taken when a configured attempt timeout expires.
    pub fn attempt_timeout_policy(
        mut self,
        value: AttemptTimeoutPolicy,
    ) -> Self {
        self.inner = self.inner.attempt_timeout_policy(value);
        self
    }
    /// Sets the grace period for cooperative worker cancellation.
    pub fn worker_cancel_grace(mut self, value: Duration) -> Self {
        self.inner = self.inner.worker_cancel_grace(value);
        self
    }
    /// Sets how Retry-After hints combine with configured delays.
    pub fn retry_after_policy(mut self, value: RetryAfterPolicy) -> Self {
        self.inner = self.inner.retry_after_policy(value);
        self
    }
    /// Validates and returns the immutable option snapshot.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when any configured option is invalid.
    pub fn build(self) -> Result<RetryOptions, RetryConfigError> {
        Ok(self.inner.build()?.options().clone())
    }
}

impl Default for RetryOptionsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
