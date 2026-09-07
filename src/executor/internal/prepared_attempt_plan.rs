// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Timed admission plan with an immutable timeout deadline.

#![cfg_attr(not(any(feature = "tokio", feature = "worker")), allow(dead_code))]

use std::time::Duration;

use qubit_clock::MonotonicInstant;

use super::prepared_timeout::PreparedTimeout;
use crate::RetryTimeoutScope;

/// One timed attempt prepared against an immutable absolute deadline.
#[derive(Clone, Copy)]
pub(crate) struct PreparedAttemptPlan {
    /// Absolute timeout transaction prepared before timer registration.
    timeout: Option<PreparedTimeout>,
}

impl PreparedAttemptPlan {
    /// Creates a prepared attempt plan from one prepared timeout transaction.
    ///
    /// # Parameters
    /// - `timeout`: Deadline, duration, and scope; None for an unbounded
    ///   attempt.
    ///
    /// # Returns
    /// An immutable plan retaining all supplied timeout data.
    #[inline]
    #[must_use]
    pub(super) fn from_timeout(timeout: Option<(MonotonicInstant, Duration, RetryTimeoutScope)>) -> Self {
        Self {
            timeout: timeout.map(|(deadline, duration, scope)| PreparedTimeout::new(deadline, duration, scope)),
        }
    }

    /// Returns the absolute timer deadline, when this attempt is bounded.
    ///
    /// # Returns
    /// `Some` containing the registered absolute deadline for a bounded
    /// attempt; None for an unbounded attempt.
    #[inline(always)]
    #[must_use]
    pub(crate) fn deadline(&self) -> Option<MonotonicInstant> {
        self.timeout.map(PreparedTimeout::deadline)
    }

    /// Returns the boundary responsible for the prepared deadline.
    ///
    /// # Returns
    /// `Some` containing the responsible timeout boundary for a bounded
    /// attempt; None for an unbounded attempt.
    #[inline(always)]
    #[must_use]
    pub(crate) fn scope(&self) -> Option<RetryTimeoutScope> {
        self.timeout.map(PreparedTimeout::scope)
    }

    /// Returns the effective duration selected at admission.
    ///
    /// # Returns
    /// `Some` containing the admission-time duration for a bounded attempt;
    /// None for an unbounded attempt.
    #[inline(always)]
    #[must_use]
    pub(super) fn duration(&self) -> Option<Duration> {
        self.timeout.map(PreparedTimeout::duration)
    }

    /// Returns the deadline and scope needed while committing a timed attempt.
    ///
    /// # Returns
    /// `Some` containing the deadline and its source for a bounded attempt;
    /// None for an unbounded attempt.
    #[inline(always)]
    #[must_use]
    pub(super) fn deadline_and_scope(&self) -> Option<(MonotonicInstant, RetryTimeoutScope)> {
        self.timeout.map(|timeout| (timeout.deadline(), timeout.scope()))
    }
}
