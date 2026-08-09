// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Inputs to one backoff calculation.

use std::time::Duration;

/// Optional caller-provided delay information for one scheduled retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffRequest {
    pub(crate) explicit_delay: Option<Duration>,
    pub(crate) hint: Option<Duration>,
    pub(crate) jitter_hint: bool,
}

impl BackoffRequest {
    /// Uses only the policy strategy.
    pub fn policy() -> Self {
        Self {
            explicit_delay: None,
            hint: None,
            jitter_hint: false,
        }
    }

    /// Supplies a server or application delay hint.
    pub fn hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            ..Self::policy()
        }
    }

    /// Supplies a hint that should also receive the policy jitter.
    pub fn jittered_hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            jitter_hint: true,
            ..Self::policy()
        }
    }

    /// Supplies an explicit rule decision. Explicit delays are exact.
    #[allow(dead_code)]
    pub(crate) fn explicit(delay: Duration) -> Self {
        Self {
            explicit_delay: Some(delay),
            ..Self::policy()
        }
    }
}
