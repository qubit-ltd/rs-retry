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
    /// One-based index assigned when this delay was selected.
    retry_index: u32,
    /// Strategy delay before caller hints, jitter and final capping.
    base_delay: Duration,
    /// Final delay after all configured transformations.
    effective_delay: Duration,
    /// Whether policy, hint or their merge selected the delay.
    source: BackoffDelaySource,
}

impl BackoffStep {
    /// Creates one calculated step.
    ///
    /// # Parameters
    /// - `retry_index`: One-based selection index.
    /// - `base_delay`: Untransformed strategy delay.
    /// - `effective_delay`: Final delay after hints, jitter and cap.
    /// - `source`: Selector responsible for the final delay.
    ///
    /// # Returns
    /// A snapshot owning the supplied delay values.
    #[inline]
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
    ///
    /// # Returns
    /// The one-based ordinal assigned when selecting this step.
    #[must_use]
    #[inline(always)]
    pub fn retry_index(&self) -> u32 {
        self.retry_index
    }

    /// Returns the strategy delay before hint/jitter resolution.
    ///
    /// # Returns
    /// The base strategy delay before hint, jitter, and final cap.
    #[must_use]
    #[inline(always)]
    pub fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// Returns the final delay to sleep.
    ///
    /// # Returns
    /// The final policy delay; an executor may further cap sleep at its
    /// hard-flow deadline.
    #[must_use]
    #[inline(always)]
    pub fn effective_delay(&self) -> Duration {
        self.effective_delay
    }

    /// Returns the stable delay source.
    ///
    /// # Returns
    /// The rule by which policy and any caller hint were combined.
    #[must_use]
    #[inline(always)]
    pub fn source(&self) -> BackoffDelaySource {
        self.source
    }
}
