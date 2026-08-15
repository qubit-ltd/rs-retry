// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private retry-after hint resolution strategy.

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Serialize;

/// Resolution policy for caller-provided retry-after hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAfterStrategy {
    /// Use the hint as the selected delay.
    PreferHint,
    /// Keep the larger of policy and hint delays.
    AtLeastBackoff,
    /// Discard the hint.
    IgnoreHint,
}

/// Stable serde representation of a retry-after resolution strategy.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetryAfterStrategyData {
    /// Use the hint as the selected delay.
    PreferHint,
    /// Keep the larger of policy and hint delays.
    AtLeastBackoff,
    /// Discard the hint.
    IgnoreHint,
}

#[cfg(feature = "serde")]
impl From<RetryAfterStrategy> for RetryAfterStrategyData {
    /// Converts runtime retry-after behavior to its stable wire representation.
    fn from(strategy: RetryAfterStrategy) -> Self {
        match strategy {
            RetryAfterStrategy::PreferHint => Self::PreferHint,
            RetryAfterStrategy::AtLeastBackoff => Self::AtLeastBackoff,
            RetryAfterStrategy::IgnoreHint => Self::IgnoreHint,
        }
    }
}

#[cfg(feature = "serde")]
impl From<RetryAfterStrategyData> for RetryAfterStrategy {
    /// Converts the stable retry-after representation to runtime behavior.
    fn from(data: RetryAfterStrategyData) -> Self {
        match data {
            RetryAfterStrategyData::PreferHint => Self::PreferHint,
            RetryAfterStrategyData::AtLeastBackoff => Self::AtLeastBackoff,
            RetryAfterStrategyData::IgnoreHint => Self::IgnoreHint,
        }
    }
}
