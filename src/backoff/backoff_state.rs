// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Mutable state for one retry or reconnect flow.

use std::sync::Arc;

use super::BackoffPolicy;
use super::BackoffRequest;
use super::BackoffStep;
use crate::RetryRandomSource;

/// Backoff state whose retry index advances once for every selected step.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_retry::BackoffPolicy;
/// use qubit_retry::BackoffRequest;
/// use qubit_retry::BackoffState;
///
/// let mut state: BackoffState = BackoffPolicy::fixed(Duration::from_millis(10)).start();
/// let step = state.next(BackoffRequest::policy());
/// assert_eq!(step.retry_index(), 1);
/// assert_eq!(step.effective_delay(), Duration::from_millis(10));
/// state.reset();
/// assert_eq!(state.retry_index(), 0);
/// ```
#[derive(Clone)]
pub struct BackoffState {
    /// Owned immutable configuration for this sequence.
    policy: BackoffPolicy,
    /// Shared sampler used for uniform selection and jitter.
    random: Arc<dyn RetryRandomSource>,
    /// Number of selected steps, saturating at u32::MAX.
    retry_index: u32,
}

impl BackoffState {
    /// Creates an empty state.
    ///
    /// # Parameters
    /// - `policy`: Owned configuration for the new sequence.
    /// - `random`: Shared sampler, retained without resetting its state.
    ///
    /// # Returns
    /// A sequence with no previously selected steps.
    #[must_use]
    #[inline]
    pub(crate) fn new(policy: BackoffPolicy, random: Arc<dyn RetryRandomSource>) -> Self {
        Self {
            policy,
            random,
            retry_index: 0,
        }
    }

    /// Returns the number of selected retry steps.
    ///
    /// # Returns
    /// Zero before the first selection or after reset, otherwise the number
    /// of selected steps, saturating at `u32::MAX`.
    #[must_use]
    #[inline(always)]
    pub fn retry_index(&self) -> u32 {
        self.retry_index
    }

    /// Calculates the next scheduled retry delay.
    ///
    /// # Parameters
    /// - `request`: Caller hint and permission to jitter it.
    ///
    /// # Returns
    /// The next source-tagged step; advances the saturating retry index once.
    #[must_use = "use the selected backoff step"]
    #[inline]
    pub fn next(&mut self, request: BackoffRequest) -> BackoffStep {
        self.retry_index = self.retry_index.saturating_add(1);
        let base_delay = self.policy.base_delay(self.retry_index, self.random.as_ref());
        self.policy
            .resolve(base_delay, request, self.retry_index, self.random.as_ref())
    }

    /// Resets the retry index after a stable connection or completed flow.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.retry_index = 0;
    }
}
