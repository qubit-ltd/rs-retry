// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Absolute backoff deadline prepared by the flow controller.

use qubit_clock::MonotonicInstant;

/// Backoff registration data shared by all executor facades.
pub(crate) struct PreparedBackoffPlan {
    /// Absolute deadline at which the selected backoff ends.
    deadline: MonotonicInstant,
    immediate: bool,
}

impl PreparedBackoffPlan {
    /// Creates a plan with an absolute deadline.
    #[inline]
    pub(crate) fn new(deadline: MonotonicInstant, immediate: bool) -> Self {
        Self {
            deadline,
            immediate,
        }
    }

    /// Returns the absolute deadline that was prepared for backoff.
    #[inline]
    pub(crate) fn deadline(&self) -> MonotonicInstant {
        self.deadline
    }

    #[inline]
    pub(crate) fn is_immediate(&self) -> bool {
        self.immediate
    }
}
