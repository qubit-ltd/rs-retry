// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Result of one backoff calculation.

use std::time::Duration;

use super::backoff_delay_source::BackoffDelaySource;

/// One immutable result from [`crate::BackoffState::next`].
#[must_use]
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
    #[must_use]
    pub fn retry_index(&self) -> u32 {
        self.retry_index
    }

    /// Returns the strategy delay before hint/jitter resolution.
    #[must_use]
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// Returns the final delay to sleep.
    #[must_use]
    pub fn effective_delay(&self) -> Duration {
        self.effective_delay
    }

    /// Returns the stable delay source.
    #[must_use]
    pub fn source(&self) -> BackoffDelaySource {
        self.source
    }
}
