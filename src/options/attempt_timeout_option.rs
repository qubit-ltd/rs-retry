// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Per-attempt timeout option.

use std::time::Duration;

use qubit_argument::{ArgumentResult, require_that};
use serde::{Deserialize, Serialize};

use super::attempt_timeout_policy::AttemptTimeoutPolicy;
use crate::error::argument_error_message;

/// Per-attempt timeout settings.
///
/// A timeout option combines the timeout duration with the policy selected when
/// an attempt exceeds that duration. Runtime constructors retain the native
/// [`Duration`] precision. Serde interchange stores half-up rounded whole
/// milliseconds, and deserialization does not automatically call
/// [`AttemptTimeoutOption::validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTimeoutOption {
    /// Timeout applied to each eligible attempt. Serde stores this value as
    /// half-up rounded whole milliseconds.
    #[serde(with = "qubit_datatype::serde::duration_millis")]
    timeout: Duration,
    /// Policy used when the attempt times out.
    policy: AttemptTimeoutPolicy,
}

impl AttemptTimeoutOption {
    /// Creates a per-attempt timeout option.
    ///
    /// # Arguments
    /// - `timeout`: Maximum duration for one attempt.
    /// - `policy`: Action selected when the timeout is reached.
    ///
    /// # Returns
    /// A timeout option. Call [`AttemptTimeoutOption::validate`] before using
    /// values that come from configuration or user input.
    #[inline(always)]
    pub fn new(timeout: Duration, policy: AttemptTimeoutPolicy) -> Self {
        Self { timeout, policy }
    }

    /// Creates a timeout option that retries timed-out attempts.
    ///
    /// # Arguments
    /// - `timeout`: Maximum duration for one attempt.
    ///
    /// # Returns
    /// A timeout option using [`AttemptTimeoutPolicy::Retry`].
    #[inline(always)]
    pub fn retry(timeout: Duration) -> Self {
        Self::new(timeout, AttemptTimeoutPolicy::Retry)
    }

    /// Creates a timeout option that aborts on the first timed-out attempt.
    ///
    /// # Arguments
    /// - `timeout`: Maximum duration for one attempt.
    ///
    /// # Returns
    /// A timeout option using [`AttemptTimeoutPolicy::Abort`].
    #[inline(always)]
    pub fn abort(timeout: Duration) -> Self {
        Self::new(timeout, AttemptTimeoutPolicy::Abort)
    }

    /// Returns a copy with another timeout policy.
    ///
    /// # Arguments
    /// - `policy`: Replacement timeout policy.
    ///
    /// # Returns
    /// A timeout option with the same duration and the new policy.
    #[inline(always)]
    pub fn with_policy(self, policy: AttemptTimeoutPolicy) -> Self {
        Self { policy, ..self }
    }

    /// Returns the timeout duration.
    ///
    /// # Returns
    /// Maximum duration allowed for one attempt.
    #[inline(always)]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the timeout policy.
    ///
    /// # Returns
    /// Policy selected when one attempt times out.
    #[inline(always)]
    pub fn policy(&self) -> AttemptTimeoutPolicy {
        self.policy
    }

    /// Validates this timeout option.
    ///
    /// # Returns
    /// `Ok(())` when the timeout can be used by an executor.
    ///
    /// # Errors
    /// Returns an error when the timeout duration is zero.
    pub fn validate(&self) -> Result<(), String> {
        (*self)
            .validate_argument("attempt_timeout")
            .map(|_| ())
            .map_err(argument_error_message)
    }

    /// Validates this timeout with structured argument error context.
    ///
    /// # Arguments
    /// - `path`: Configuration path associated with the timeout.
    ///
    /// # Returns
    /// The unchanged timeout option when its duration is positive.
    ///
    /// # Errors
    /// Returns an argument error at `path` when the timeout is zero.
    pub(super) fn validate_argument(self, path: &str) -> ArgumentResult<Self> {
        require_that(
            self.timeout,
            path,
            |timeout| !timeout.is_zero(),
            "positive",
            "attempt timeout must be greater than zero",
        )?;
        Ok(self)
    }
}
