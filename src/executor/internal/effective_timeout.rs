// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Source-aware effective timeout selection.

use std::time::Duration;

use crate::AttemptTimeoutKind;

/// Effective hard timeout selected for one attempt.
#[derive(Clone, Copy)]
pub(crate) struct EffectiveTimeout {
    /// Duration until cancellation.
    duration: Duration,
    /// Boundary responsible for cancellation.
    kind: AttemptTimeoutKind,
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
                kind: AttemptTimeoutKind::Attempt,
            }),
            (Some(_), Some(flow)) => Some(Self {
                duration: flow,
                kind: AttemptTimeoutKind::Flow,
            }),
            (Some(attempt), None) => Some(Self {
                duration: attempt,
                kind: AttemptTimeoutKind::Attempt,
            }),
            (None, Some(flow)) => Some(Self {
                duration: flow,
                kind: AttemptTimeoutKind::Flow,
            }),
            (None, None) => None,
        }
    }

    /// Returns the selected duration.
    pub(crate) fn duration(self) -> Duration {
        self.duration
    }

    /// Returns the boundary responsible for cancellation.
    pub(crate) fn kind(self) -> AttemptTimeoutKind {
        self.kind
    }
}
