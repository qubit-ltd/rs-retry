// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Decisions returned by retry rules.

use std::time::Duration;

/// Decision returned by one ordered retry rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[derive(Default)]
pub enum RetryDecision {
    /// Let the next rule or built-in default decide.
    #[default]
    UseDefault,
    /// Schedule the next retry using the policy backoff.
    Retry,
    /// Schedule the next retry with this exact delay.
    RetryAfter(Duration),
    /// Stop the retry flow immediately.
    Abort,
}
