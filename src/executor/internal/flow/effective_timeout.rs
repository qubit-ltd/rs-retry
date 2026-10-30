// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Source-aware effective timeout selection.

#![cfg_attr(not(any(feature = "async", feature = "worker")), allow(dead_code))]

use std::time::Duration;

use crate::RetryTimeoutScope;

/// Hard timeout and cancellation boundary selected for one attempt.
#[derive(Clone, Copy)]
pub(crate) struct EffectiveTimeout {
    /// Maximum duration before this timeout cancels the attempt.
    duration: Duration,
    /// Whether the attempt or complete retry flow supplied the selected limit.
    scope: RetryTimeoutScope,
}

impl EffectiveTimeout {
    /// Selects the shorter configured hard timeout and retains its source.
    ///
    /// The per-attempt timeout wins exact ties. A missing limit does not
    /// constrain selection; when both limits are missing, no timeout is
    /// selected.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Remaining per-attempt limit, or `None` if unset.
    /// - `flow_remaining`: Remaining hard-flow limit, or `None` if unset.
    ///
    /// # Returns
    /// `Some` with the shorter configured limit and its source, or `None` if
    /// neither limit is configured.
    pub(crate) fn select(attempt_timeout: Option<Duration>, flow_remaining: Option<Duration>) -> Option<Self> {
        match (attempt_timeout, flow_remaining) {
            (Some(attempt), Some(flow)) if attempt <= flow => Some(Self {
                duration: attempt,
                scope: RetryTimeoutScope::Attempt,
            }),
            (Some(_), Some(flow)) => Some(Self {
                duration: flow,
                scope: RetryTimeoutScope::Flow,
            }),
            (Some(attempt), None) => Some(Self {
                duration: attempt,
                scope: RetryTimeoutScope::Attempt,
            }),
            (None, Some(flow)) => Some(Self {
                duration: flow,
                scope: RetryTimeoutScope::Flow,
            }),
            (None, None) => None,
        }
    }

    /// Returns the selected duration before the timeout boundary is reached.
    ///
    /// # Returns
    /// The selected hard timeout duration.
    #[must_use]
    #[inline]
    pub(crate) fn duration(self) -> Duration {
        self.duration
    }

    /// Returns which configured timeout supplied the selected duration.
    ///
    /// # Returns
    /// The boundary that wins, with Attempt winning exact ties.
    #[must_use]
    #[inline]
    pub(crate) fn scope(self) -> RetryTimeoutScope {
        self.scope
    }
}
