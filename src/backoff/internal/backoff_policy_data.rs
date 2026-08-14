// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Unvalidated serde representation for a backoff policy.

use serde::Deserialize;
use serde::Serialize;

use super::BackoffStrategy;
use super::JitterStrategy;
use super::RetryAfterStrategy;

/// Raw policy data validated before it becomes a [`crate::BackoffPolicy`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackoffPolicyData {
    /// Base-delay strategy.
    pub strategy: BackoffStrategy,
    /// Jitter strategy.
    pub jitter: JitterStrategy,
    /// Retry-after hint strategy.
    pub retry_after: RetryAfterStrategy,
}
