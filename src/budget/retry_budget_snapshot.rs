// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Defines immutable retry budget observations.

use std::time::Duration;

/// A coherent observation of a retry budget.
///
/// `total_elapsed` is sampled when this value is created. `operation_elapsed`
/// includes only durations completed through
/// [`super::RetryBudget::finish_attempt`].
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryBudgetSnapshot {
    /// Number of attempts admitted so far.
    attempts: u32,

    /// Accumulated explicit operation duration.
    operation_elapsed: Duration,

    /// End-to-end elapsed time at the observation sample.
    total_elapsed: Duration,

    /// Duration of the most recently finished attempt.
    attempt_elapsed: Duration,
}

impl RetryBudgetSnapshot {
    /// Creates a snapshot from state owned by the retry budget.
    pub(super) const fn new(
        attempts: u32,
        operation_elapsed: Duration,
        total_elapsed: Duration,
        attempt_elapsed: Duration,
    ) -> Self {
        Self {
            attempts,
            operation_elapsed,
            total_elapsed,
            attempt_elapsed,
        }
    }

    /// Returns the number of admitted attempts.
    #[inline(always)]
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Returns elapsed time accumulated across completed operations.
    #[inline(always)]
    #[must_use]
    pub const fn operation_elapsed(&self) -> Duration {
        self.operation_elapsed
    }

    /// Returns the total elapsed time sampled with this snapshot.
    #[inline(always)]
    #[must_use]
    pub const fn total_elapsed(&self) -> Duration {
        self.total_elapsed
    }

    /// Returns elapsed time for the most recently completed attempt.
    #[inline(always)]
    #[must_use]
    pub const fn attempt_elapsed(&self) -> Duration {
        self.attempt_elapsed
    }
}
