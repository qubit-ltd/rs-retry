// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Decisions returned by retry rules.

use std::time::Duration;

/// Decision returned by one ordered retry rule.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[derive(Default)]
pub enum RetryDecision {
    /// Let the next rule or built-in default decide.
    #[default]
    UseDefault,
    /// Schedule the next retry using the policy backoff.
    Retry,
    /// Schedule the next retry with an unjittered server or application hint.
    RetryWithHint(Duration),
    /// Schedule the next retry with a jittered server or application hint.
    RetryWithJitteredHint(Duration),
    /// Stop the retry flow immediately.
    Abort,
}

impl RetryDecision {
    /// Returns the caller-provided delay hint carried by this decision.
    pub(crate) fn retry_after_hint(self) -> Option<Duration> {
        match self {
            Self::RetryWithHint(hint) | Self::RetryWithJitteredHint(hint) => {
                Some(hint)
            }
            Self::UseDefault | Self::Retry | Self::Abort => None,
        }
    }
}
