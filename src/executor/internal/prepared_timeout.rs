// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable timeout data prepared during async admission.

use std::time::Duration;

use qubit_clock::MonotonicInstant;

use crate::RetryTimeoutScope;

/// Absolute timeout selected from one coherent admission clock sample.
#[derive(Clone, Copy)]
pub(in crate::executor::internal) struct PreparedTimeout {
    /// Fixed deadline registered with the async timer.
    deadline: MonotonicInstant,
    /// Effective duration at the admission sample.
    duration: Duration,
    /// Boundary responsible for the fixed deadline.
    scope: RetryTimeoutScope,
}

impl PreparedTimeout {
    /// Creates immutable timeout data from an admission transaction.
    pub(super) fn new(
        deadline: MonotonicInstant,
        duration: Duration,
        scope: RetryTimeoutScope,
    ) -> Self {
        Self {
            deadline,
            duration,
            scope,
        }
    }

    /// Returns the fixed deadline.
    pub(super) fn deadline(self) -> MonotonicInstant {
        self.deadline
    }

    /// Returns the selected duration.
    pub(super) fn duration(self) -> Duration {
        self.duration
    }

    /// Returns the boundary responsible for the deadline.
    pub(super) fn scope(self) -> RetryTimeoutScope {
        self.scope
    }
}
