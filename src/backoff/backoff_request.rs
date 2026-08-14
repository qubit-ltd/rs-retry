// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Inputs to one backoff calculation.

use std::time::Duration;

/// Optional caller-provided delay information for one scheduled retry.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffRequest {
    pub(crate) hint: Option<Duration>,
    pub(crate) jitter_hint: bool,
}

impl BackoffRequest {
    /// Uses only the policy strategy.
    #[must_use = "use the request when calculating a backoff step"]
    pub fn policy() -> Self {
        Self {
            hint: None,
            jitter_hint: false,
        }
    }

    /// Supplies a server or application delay hint.
    #[must_use = "use the request when calculating a backoff step"]
    pub fn hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            ..Self::policy()
        }
    }

    /// Supplies a hint that should also receive the policy jitter.
    #[must_use = "use the request when calculating a backoff step"]
    pub fn jittered_hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            jitter_hint: true,
        }
    }
}
