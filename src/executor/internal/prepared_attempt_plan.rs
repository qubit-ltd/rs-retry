// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Async admission plan with an immutable timeout deadline.

use std::time::Duration;

use qubit_clock::MonotonicInstant;

use super::prepared_timeout::PreparedTimeout;
use crate::RetryTimeoutScope;

/// One async attempt prepared against an immutable absolute deadline.
#[derive(Clone, Copy)]
pub(crate) struct PreparedAttemptPlan {
    /// Absolute timeout transaction prepared before timer registration.
    timeout: Option<PreparedTimeout>,
}

impl PreparedAttemptPlan {
    /// Creates a prepared attempt plan from one admitted timeout transaction.
    pub(super) fn from_timeout(
        timeout: Option<(MonotonicInstant, Duration, RetryTimeoutScope)>,
    ) -> Self {
        Self {
            timeout: timeout.map(|(deadline, duration, scope)| {
                PreparedTimeout::new(deadline, duration, scope)
            }),
        }
    }

    /// Returns the absolute timer deadline, when this attempt is bounded.
    pub(crate) fn deadline(&self) -> Option<MonotonicInstant> {
        self.timeout.map(PreparedTimeout::deadline)
    }

    /// Returns the boundary responsible for the prepared deadline.
    pub(crate) fn scope(&self) -> Option<RetryTimeoutScope> {
        self.timeout.map(PreparedTimeout::scope)
    }

    /// Returns the effective duration selected at admission.
    pub(super) fn duration(&self) -> Option<Duration> {
        self.timeout.map(PreparedTimeout::duration)
    }

    /// Returns the deadline and scope needed while committing an async attempt.
    pub(super) fn deadline_and_scope(
        &self,
    ) -> Option<(MonotonicInstant, RetryTimeoutScope)> {
        self.timeout
            .map(|timeout| (timeout.deadline(), timeout.scope()))
    }
}
