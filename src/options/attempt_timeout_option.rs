/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Per-attempt timeout option.
//!

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::attempt_timeout_policy::AttemptTimeoutPolicy;

/// Per-attempt timeout settings.
///
/// A timeout option combines the timeout duration with the policy selected when
/// an attempt exceeds that duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTimeoutOption {
    /// Timeout applied to each eligible attempt.
    #[serde(with = "qubit_serde::serde::duration_millis")]
    timeout: Duration,
    /// Policy used when the attempt times out.
    policy: AttemptTimeoutPolicy,
}

impl AttemptTimeoutOption {
    /// Creates a per-attempt timeout option.
    ///
    /// # Parameters
    /// - `timeout`: Maximum duration for one attempt.
    /// - `policy`: Action selected when the timeout is reached.
    ///
    /// # Returns
    /// A timeout option. Call [`AttemptTimeoutOption::validate`] before using
    /// values that come from configuration or user input.
    #[inline]
    pub fn new(timeout: Duration, policy: AttemptTimeoutPolicy) -> Self {
        Self { timeout, policy }
    }

    /// Creates a timeout option that retries timed-out attempts.
    ///
    /// # Parameters
    /// - `timeout`: Maximum duration for one attempt.
    ///
    /// # Returns
    /// A timeout option using [`AttemptTimeoutPolicy::Retry`].
    #[inline]
    pub fn retry(timeout: Duration) -> Self {
        Self::new(timeout, AttemptTimeoutPolicy::Retry)
    }

    /// Creates a timeout option that aborts on the first timed-out attempt.
    ///
    /// # Parameters
    /// - `timeout`: Maximum duration for one attempt.
    ///
    /// # Returns
    /// A timeout option using [`AttemptTimeoutPolicy::Abort`].
    #[inline]
    pub fn abort(timeout: Duration) -> Self {
        Self::new(timeout, AttemptTimeoutPolicy::Abort)
    }

    /// Returns the timeout duration.
    ///
    /// # Returns
    /// Maximum duration allowed for one attempt.
    #[inline]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the timeout policy.
    ///
    /// # Returns
    /// Policy selected when one attempt times out.
    #[inline]
    pub fn policy(&self) -> AttemptTimeoutPolicy {
        self.policy
    }

    /// Returns a copy with another timeout policy.
    ///
    /// # Parameters
    /// - `policy`: Replacement timeout policy.
    ///
    /// # Returns
    /// A timeout option with the same duration and the new policy.
    #[inline]
    pub fn with_policy(self, policy: AttemptTimeoutPolicy) -> Self {
        Self { policy, ..self }
    }

    /// Validates this timeout option.
    ///
    /// # Returns
    /// `Ok(())` when the timeout can be used by an executor.
    ///
    /// # Errors
    /// Returns an error when the timeout duration is zero.
    pub fn validate(&self) -> Result<(), String> {
        if self.timeout.is_zero() {
            Err("attempt timeout must be greater than zero".to_string())
        } else {
            Ok(())
        }
    }
}
