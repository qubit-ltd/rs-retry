// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Source-aware effective timeout selection.

use std::time::Duration;

use crate::RetryTimeoutScope;

/// Effective hard timeout selected for one attempt.
#[derive(Clone, Copy)]
pub(crate) struct EffectiveTimeout {
    /// Duration until cancellation.
    duration: Duration,
    /// Boundary responsible for cancellation.
    scope: RetryTimeoutScope,
}

impl EffectiveTimeout {
    /// Selects the shorter hard timeout while retaining its source.
    ///
    /// An exact tie is attributed to the configured attempt timeout.
    pub(crate) fn select(
        attempt_timeout: Option<Duration>,
        flow_remaining: Option<Duration>,
    ) -> Option<Self> {
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

    /// Returns the selected duration.
    #[must_use]
    pub(crate) fn duration(self) -> Duration {
        self.duration
    }

    /// Returns the boundary responsible for cancellation.
    #[must_use]
    pub(crate) fn scope(self) -> RetryTimeoutScope {
        self.scope
    }
}
