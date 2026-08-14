// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Borrowed monotonic clock adapter for time budgets.

use std::sync::Arc;

use qubit_clock::ClockDomain;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;

/// Borrows a clock without requiring a blanket implementation for references.
pub struct BorrowedMonotonicClock<'a>(pub &'a dyn MonotonicClock);

impl MonotonicClock for BorrowedMonotonicClock<'_> {
    /// Returns the borrowed clock's domain.
    fn domain(&self) -> ClockDomain {
        self.0.domain()
    }

    /// Returns the borrowed clock's current instant.
    fn now(&self) -> MonotonicInstant {
        self.0.now()
    }

    /// Calculates a deadline using the borrowed clock.
    fn deadline_after(
        &self,
        duration: std::time::Duration,
    ) -> Result<MonotonicInstant, TimeError> {
        self.0.deadline_after(duration)
    }

    /// Creates a timer using the borrowed clock.
    fn new_timer(&self) -> Arc<dyn Timer> {
        self.0.new_timer()
    }
}
