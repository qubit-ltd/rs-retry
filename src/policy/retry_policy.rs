// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable retry policy.

use serde::Deserialize;
use serde::Serialize;

use super::RetryLimits;
use super::RetryPolicyBuilder;
use crate::backoff::BackoffPolicy;

/// Pure retry limits and backoff configuration.
#[must_use]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    limits: RetryLimits,
    backoff: BackoffPolicy,
}

impl RetryPolicy {
    /// Creates a policy builder.
    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::new()
    }

    /// Creates a policy from validated components.
    pub(crate) fn new(limits: RetryLimits, backoff: BackoffPolicy) -> Self {
        Self { limits, backoff }
    }

    /// Returns retry continuation limits.
    #[must_use = "inspect the retry limits"]
    pub fn limits(&self) -> &RetryLimits {
        &self.limits
    }

    /// Returns the immutable backoff configuration.
    #[must_use = "inspect the backoff configuration"]
    pub fn backoff(&self) -> &BackoffPolicy {
        &self.backoff
    }
}
