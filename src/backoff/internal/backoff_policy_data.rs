// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Unvalidated serde representation for a backoff policy.

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "serde")]
use super::BackoffStrategyData;
#[cfg(feature = "serde")]
use super::JitterStrategyData;
#[cfg(feature = "serde")]
use super::RetryAfterStrategyData;

/// Raw policy data validated before it becomes a [`crate::BackoffPolicy`].
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackoffPolicyData {
    /// Base-delay strategy.
    pub strategy: BackoffStrategyData,
    /// Jitter strategy.
    pub jitter: JitterStrategyData,
    /// Retry-after hint strategy.
    pub retry_after: RetryAfterStrategyData,
}
