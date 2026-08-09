// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Retry continuation budgets.

use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

/// Limits that decide whether a retry flow may continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryLimits {
    max_attempts: NonZeroU32,
    max_operation_elapsed: Option<Duration>,
    max_total_elapsed: Option<Duration>,
}

impl RetryLimits {
    /// Creates validated retry limits.
    pub(crate) fn new(
        max_attempts: NonZeroU32,
        max_operation_elapsed: Option<Duration>,
        max_total_elapsed: Option<Duration>,
    ) -> Self {
        Self {
            max_attempts,
            max_operation_elapsed,
            max_total_elapsed,
        }
    }

    /// Returns the maximum number of attempts, including the first attempt.
    pub fn max_attempts(&self) -> NonZeroU32 {
        self.max_attempts
    }

    /// Returns the cumulative operation-time budget.
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns the whole-flow wall-clock budget.
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }
}
