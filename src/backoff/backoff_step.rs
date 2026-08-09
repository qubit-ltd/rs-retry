// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Result of one backoff calculation.

use std::time::Duration;

/// Stable source classification for a selected delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffDelaySource {
    /// Delay selected solely from the configured policy.
    Policy,
    /// Delay explicitly selected by a retry rule.
    Explicit,
    /// Delay selected from a hint.
    Hint,
    /// Delay formed by merging a hint and policy delay.
    Merged,
}

/// One immutable result from [`crate::BackoffState::next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffStep {
    retry_index: u32,
    base_delay: Duration,
    effective_delay: Duration,
    source: BackoffDelaySource,
}

impl BackoffStep {
    /// Creates one calculated step.
    pub(crate) fn new(
        retry_index: u32,
        base_delay: Duration,
        effective_delay: Duration,
        source: BackoffDelaySource,
    ) -> Self {
        Self {
            retry_index,
            base_delay,
            effective_delay,
            source,
        }
    }

    /// Returns the one-based retry index.
    pub fn retry_index(&self) -> u32 {
        self.retry_index
    }

    /// Returns the strategy delay before hint/jitter resolution.
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// Returns the final delay to sleep.
    pub fn effective_delay(&self) -> Duration {
        self.effective_delay
    }

    /// Returns the stable delay source.
    pub fn source(&self) -> BackoffDelaySource {
        self.source
    }
}
