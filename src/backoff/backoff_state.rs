// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Mutable state for one retry or reconnect flow.

use std::sync::Arc;

use super::BackoffPolicy;
use super::BackoffRequest;
use super::BackoffStep;
use crate::RetryRandomSource;

/// Backoff state whose retry index advances once for every selected step.
#[derive(Clone)]
pub struct BackoffState {
    policy: BackoffPolicy,
    random: Arc<dyn RetryRandomSource>,
    retry_index: u32,
}

impl BackoffState {
    /// Creates an empty state.
    pub(crate) fn new(
        policy: BackoffPolicy,
        random: Arc<dyn RetryRandomSource>,
    ) -> Self {
        Self {
            policy,
            random,
            retry_index: 0,
        }
    }

    /// Calculates the next scheduled retry delay.
    pub fn next(&mut self, request: BackoffRequest) -> BackoffStep {
        self.retry_index = self.retry_index.saturating_add(1);
        let base_delay = self
            .policy
            .base_delay(self.retry_index, self.random.as_ref());
        self.policy.resolve(
            base_delay,
            request,
            self.retry_index,
            self.random.as_ref(),
        )
    }

    /// Resets the retry index after a stable connection or completed flow.
    pub fn reset(&mut self) {
        self.retry_index = 0;
    }

    /// Returns the number of selected retry steps.
    pub fn retry_index(&self) -> u32 {
        self.retry_index
    }
}
