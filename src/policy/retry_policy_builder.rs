// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder for [`super::RetryPolicy`].

use std::num::NonZeroU32;
use std::time::Duration;

use super::RetryLimits;
use super::RetryPolicy;
use crate::RetryPolicyError;
use crate::backoff::BackoffPolicy;

/// Validated builder for a pure retry policy.
#[derive(Debug, Clone)]
#[must_use]
pub struct RetryPolicyBuilder {
    max_attempts: u32,
    max_operation_elapsed: Option<Duration>,
    max_total_elapsed: Option<Duration>,
    backoff: BackoffPolicy,
}

impl RetryPolicyBuilder {
    /// Creates a builder with three attempts and immediate retries.
    pub fn new() -> Self {
        Self {
            max_attempts: 3,
            max_operation_elapsed: None,
            max_total_elapsed: None,
            backoff: BackoffPolicy::immediate(),
        }
    }

    /// Sets the maximum number of attempts, including the first attempt.
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Sets the cumulative operation-time budget.
    pub fn max_operation_elapsed(mut self, elapsed: Duration) -> Self {
        self.max_operation_elapsed = Some(elapsed);
        self
    }

    /// Sets or removes the cumulative operation-time budget.
    pub fn max_operation_elapsed_opt(
        mut self,
        elapsed: Option<Duration>,
    ) -> Self {
        self.max_operation_elapsed = elapsed;
        self
    }

    /// Removes the cumulative operation-time budget.
    pub fn without_operation_elapsed(mut self) -> Self {
        self.max_operation_elapsed = None;
        self
    }

    /// Sets the whole-flow wall-clock budget.
    pub fn max_total_elapsed(mut self, elapsed: Duration) -> Self {
        self.max_total_elapsed = Some(elapsed);
        self
    }

    /// Sets or removes the whole-flow budget.
    pub fn max_total_elapsed_opt(mut self, elapsed: Option<Duration>) -> Self {
        self.max_total_elapsed = elapsed;
        self
    }

    /// Removes the whole-flow wall-clock budget.
    pub fn without_total_elapsed(mut self) -> Self {
        self.max_total_elapsed = None;
        self
    }

    /// Sets the pure backoff policy.
    pub fn backoff(mut self, backoff: BackoffPolicy) -> Self {
        self.backoff = backoff;
        self
    }

    /// Validates and creates the retry policy.
    pub fn build(self) -> Result<RetryPolicy, RetryPolicyError> {
        let max_attempts =
            NonZeroU32::new(self.max_attempts).ok_or_else(|| {
                RetryPolicyError::new(
                    "max_attempts",
                    "maximum attempts must be greater than zero",
                )
            })?;
        Ok(RetryPolicy::new(
            RetryLimits::new(
                max_attempts,
                self.max_operation_elapsed,
                self.max_total_elapsed,
            ),
            self.backoff,
        ))
    }
}

impl Default for RetryPolicyBuilder {
    fn default() -> Self {
        Self::new()
    }
}
