// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Defines the linear token for one admitted retry attempt.

use qubit_clock::MonotonicInstant;

/// A single admitted retry attempt.
///
/// A value is returned by [`super::RetryBudget::begin_attempt`] and must be
/// consumed by [`super::RetryBudget::finish_attempt`]. It intentionally is not
/// clonable, so one attempt cannot be finished more than once.
#[must_use]
#[derive(Debug)]
pub struct RetryAttempt {
    /// One-based ordinal assigned when this attempt was admitted.
    pub(super) number: u32,

    /// Monotonic instant sampled immediately after admission.
    pub(super) started_at: MonotonicInstant,
}

impl RetryAttempt {
    /// Returns the one-based ordinal of this admitted attempt.
    #[inline(always)]
    pub const fn number(&self) -> u32 {
        self.number
    }
}
