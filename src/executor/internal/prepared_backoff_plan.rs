// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Absolute backoff deadline prepared by the flow controller.

use qubit_clock::MonotonicInstant;

/// Backoff registration data shared by all executor facades.
pub(crate) struct PreparedBackoffPlan {
    deadline: MonotonicInstant,
}

impl PreparedBackoffPlan {
    #[inline]
    pub(crate) fn new(deadline: MonotonicInstant) -> Self {
        Self { deadline }
    }

    #[inline]
    pub(crate) fn deadline(&self) -> MonotonicInstant {
        self.deadline
    }
}
