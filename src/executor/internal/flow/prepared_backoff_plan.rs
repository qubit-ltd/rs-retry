// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Absolute backoff deadline prepared by the flow controller.

use qubit_clock::MonotonicInstant;

/// Backoff registration data shared by executor facades.
///
/// The flow controller prepares the absolute deadline and whether the
/// effective delay is zero. Executor facades must consume this plan to
/// register the timer or proceed immediately.
#[must_use = "a prepared backoff plan must be registered or handled immediately"]
pub(crate) struct PreparedBackoffPlan {
    /// Absolute deadline at which the selected backoff ends.
    deadline: MonotonicInstant,
    /// Whether the selected backoff has no delay and can proceed immediately.
    immediate: bool,
}

impl PreparedBackoffPlan {
    /// Creates a plan with an absolute deadline.
    ///
    /// # Parameters
    /// - `deadline`: Absolute deadline in the execution clock domain.
    /// - `immediate`: Whether the selected backoff has no delay.
    ///
    /// # Returns
    /// A plan that the executor must register or handle immediately.
    #[inline]
    pub(crate) fn new(deadline: MonotonicInstant, immediate: bool) -> Self {
        Self { deadline, immediate }
    }

    /// Returns the absolute deadline that was prepared for backoff.
    ///
    /// # Returns
    /// The unchanged absolute deadline in the execution clock domain.
    #[inline]
    pub(crate) fn deadline(&self) -> MonotonicInstant {
        self.deadline
    }

    /// Reports whether the selected backoff can proceed without waiting.
    ///
    /// # Returns
    /// `true` when the effective backoff delay is zero.
    #[inline]
    #[must_use]
    pub(crate) fn is_immediate(&self) -> bool {
        self.immediate
    }
}
