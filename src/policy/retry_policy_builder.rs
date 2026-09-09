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

use super::RetryAdmissionLimits;
use super::RetryPolicy;
use crate::RetryPolicyError;
use crate::backoff::BackoffPolicy;

/// Validated builder for a pure retry policy.
#[derive(Debug, Clone)]
#[must_use]
pub struct RetryPolicyBuilder {
    /// Maximum admissions including the initial operation.
    max_attempts: u32,
    /// Optional cumulative operation-time limit; None disables this soft
    /// budget.
    operation_time_budget: Option<Duration>,
    /// Optional monotonic whole-flow limit; None disables this soft budget.
    total_time_budget: Option<Duration>,
    /// Validated delay configuration copied into each fresh flow.
    backoff: BackoffPolicy,
}

impl RetryPolicyBuilder {
    /// Creates a builder initialized from a validated policy.
    ///
    /// # Parameters
    /// - `policy`: Validated policy whose limits and backoff are copied.
    ///
    /// # Returns
    /// A builder that reproduces the supplied policy on build.
    #[inline]
    pub(crate) fn from_policy(policy: RetryPolicy) -> Self {
        let limits = policy.admission_limits();
        let mut builder = Self::new()
            .max_attempts(limits.max_attempts().get())
            .backoff(policy.backoff().clone());
        if let Some(budget) = limits.operation_time_budget() {
            builder = builder.operation_time_budget(budget);
        } else {
            builder = builder.without_operation_time_budget();
        }
        if let Some(budget) = limits.total_time_budget() {
            builder = builder.total_time_budget(budget);
        } else {
            builder = builder.without_total_time_budget();
        }
        builder
    }

    /// Creates a builder with three attempts and immediate retries.
    ///
    /// # Returns
    /// A builder with three total attempts, no elapsed limits, and immediate
    /// backoff.
    #[inline]
    pub fn new() -> Self {
        Self {
            max_attempts: 3,
            operation_time_budget: None,
            total_time_budget: None,
            backoff: BackoffPolicy::immediate(),
        }
    }

    /// Sets the maximum number of attempts, including the first attempt.
    ///
    /// # Parameters
    /// - `max_attempts`: Total admissions including the initial attempt; build
    ///   rejects zero.
    ///
    /// # Returns
    /// The owned builder retaining the requested count.
    #[inline(always)]
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Sets the cumulative operation-time budget.
    ///
    /// # Parameters
    /// - `elapsed`: Soft cumulative operation budget; zero prohibits admission.
    ///
    /// # Returns
    /// The owned builder with that budget enabled.
    #[inline(always)]
    pub fn operation_time_budget(mut self, elapsed: Duration) -> Self {
        self.operation_time_budget = Some(elapsed);
        self
    }

    /// Sets or removes the cumulative operation-time budget.
    ///
    /// # Parameters
    /// - `elapsed`: Some soft cumulative operation budget, or None to remove
    ///   it.
    ///
    /// # Returns
    /// The owned builder with the optional budget replaced.
    #[inline(always)]
    pub fn operation_time_budget_opt(
        mut self,
        elapsed: Option<Duration>,
    ) -> Self {
        self.operation_time_budget = elapsed;
        self
    }

    /// Removes the cumulative operation-time budget.
    ///
    /// # Returns
    /// The owned builder with the corresponding elapsed budget disabled.
    #[inline(always)]
    pub fn without_operation_time_budget(mut self) -> Self {
        self.operation_time_budget = None;
        self
    }

    /// Sets the whole-flow monotonic elapsed budget.
    ///
    /// # Parameters
    /// - `elapsed`: Soft monotonic whole-flow budget; zero prohibits admission.
    ///
    /// # Returns
    /// The owned builder with that budget enabled.
    #[inline(always)]
    pub fn total_time_budget(mut self, elapsed: Duration) -> Self {
        self.total_time_budget = Some(elapsed);
        self
    }

    /// Sets or removes the whole-flow budget.
    ///
    /// # Parameters
    /// - `elapsed`: Some soft monotonic whole-flow budget, or None to remove
    ///   it.
    ///
    /// # Returns
    /// The owned builder with the optional budget replaced.
    #[inline(always)]
    pub fn total_time_budget_opt(mut self, elapsed: Option<Duration>) -> Self {
        self.total_time_budget = elapsed;
        self
    }

    /// Removes the whole-flow monotonic elapsed budget.
    ///
    /// # Returns
    /// The owned builder with the corresponding elapsed budget disabled.
    #[inline(always)]
    pub fn without_total_time_budget(mut self) -> Self {
        self.total_time_budget = None;
        self
    }

    /// Sets the pure backoff policy.
    ///
    /// # Parameters
    /// - `backoff`: Validated delay policy, consumed without starting its
    ///   sequence.
    ///
    /// # Returns
    /// The owned builder with the supplied delay configuration.
    #[inline(always)]
    pub fn backoff(mut self, backoff: BackoffPolicy) -> Self {
        self.backoff = backoff;
        self
    }

    /// Validates and creates the retry policy.
    ///
    /// # Returns
    /// A validated immutable policy owning limits and backoff.
    ///
    /// # Errors
    /// Returns a max_attempts policy error if the requested count is zero.
    #[inline]
    pub fn build(self) -> Result<RetryPolicy, RetryPolicyError> {
        let max_attempts =
            NonZeroU32::new(self.max_attempts).ok_or_else(|| {
                RetryPolicyError::new(
                    "max_attempts",
                    "maximum attempts must be greater than zero",
                )
            })?;
        Ok(RetryPolicy::new(
            RetryAdmissionLimits::new(
                max_attempts,
                self.operation_time_budget,
                self.total_time_budget,
            ),
            self.backoff,
        ))
    }
}

impl Default for RetryPolicyBuilder {
    ///
    /// Creates the same unbounded-time, three-attempt builder as `new`.
    ///
    /// # Returns
    /// A default builder ready for explicit policy customization.
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
