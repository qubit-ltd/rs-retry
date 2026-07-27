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
    AttemptTimeoutOption, AttemptTimeoutPolicy, RetryAfterPolicy, RetryBuilder, RetryConfigError,
    RetryDelay, RetryJitter, RetryOptions,
};

/// Builds a validated [`RetryOptions`] snapshot without selecting an error type.
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
    pub fn max_attempts(mut self, value: u32) -> Self {
        self.inner = self.inner.max_attempts(value);
        self
    }
    pub fn max_retries(mut self, value: u32) -> Self {
        self.inner = self.inner.max_retries(value);
        self
    }
    pub fn max_operation_elapsed(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.max_operation_elapsed(value);
        self
    }
    pub fn max_total_elapsed(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.max_total_elapsed(value);
        self
    }
    pub fn delay(mut self, value: RetryDelay) -> Self {
        self.inner = self.inner.delay(value);
        self
    }
    pub fn no_delay(mut self) -> Self {
        self.inner = self.inner.no_delay();
        self
    }
    pub fn fixed_delay(mut self, value: Duration) -> Self {
        self.inner = self.inner.fixed_delay(value);
        self
    }
    pub fn random_delay(mut self, min: Duration, max: Duration) -> Self {
        self.inner = self.inner.random_delay(min, max);
        self
    }
    pub fn exponential_backoff(mut self, initial: Duration, max: Duration) -> Self {
        self.inner = self.inner.exponential_backoff(initial, max);
        self
    }
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
    pub fn jitter(mut self, value: RetryJitter) -> Self {
        self.inner = self.inner.jitter(value);
        self
    }
    pub fn jitter_factor(mut self, value: f64) -> Self {
        self.inner = self.inner.jitter_factor(value);
        self
    }
    pub fn attempt_timeout(mut self, value: Option<Duration>) -> Self {
        self.inner = self.inner.attempt_timeout(value);
        self
    }
    pub fn attempt_timeout_option(mut self, value: Option<AttemptTimeoutOption>) -> Self {
        self.inner = self.inner.attempt_timeout_option(value);
        self
    }
    pub fn attempt_timeout_policy(mut self, value: AttemptTimeoutPolicy) -> Self {
        self.inner = self.inner.attempt_timeout_policy(value);
        self
    }
    pub fn worker_cancel_grace(mut self, value: Duration) -> Self {
        self.inner = self.inner.worker_cancel_grace(value);
        self
    }
    pub fn retry_after_policy(mut self, value: RetryAfterPolicy) -> Self {
        self.inner = self.inner.retry_after_policy(value);
        self
    }
    /// Validates and returns the immutable option snapshot.
    pub fn build(self) -> Result<RetryOptions, RetryConfigError> {
        Ok(self.inner.build()?.options().clone())
    }
}

impl Default for RetryOptionsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
