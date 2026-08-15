// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private serde wire DTO for [`crate::RetryPolicy`].

use serde::Deserialize;
use serde::Serialize;

use super::DurationData;
use crate::BackoffPolicy;
use crate::RetryLimits;
use crate::RetryPolicy;
use crate::RetryPolicyError;

/// Unvalidated policy data with the stable public wire-field layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetryPolicyData {
    /// Maximum number of attempts including the first attempt.
    pub(crate) max_attempts: u32,
    /// Optional cumulative operation budget.
    pub(crate) max_operation_elapsed: Option<DurationData>,
    /// Optional whole-flow budget.
    pub(crate) max_total_elapsed: Option<DurationData>,
    /// Validated backoff strategy data.
    pub(crate) backoff: BackoffPolicy,
}

impl From<&RetryPolicy> for RetryPolicyData {
    /// Copies a policy into its stable public wire-field layout.
    fn from(policy: &RetryPolicy) -> Self {
        let limits = policy.limits();
        Self {
            max_attempts: limits.max_attempts().get(),
            max_operation_elapsed: limits
                .max_operation_elapsed()
                .map(DurationData::from),
            max_total_elapsed: limits
                .max_total_elapsed()
                .map(DurationData::from),
            backoff: policy.backoff().clone(),
        }
    }
}

impl TryFrom<RetryPolicyData> for RetryPolicy {
    type Error = RetryPolicyError;

    /// Converts wire data through the same limits validation as the builder.
    fn try_from(data: RetryPolicyData) -> Result<Self, Self::Error> {
        let limits = RetryLimits::try_from(super::RetryLimitsData {
            max_attempts: data.max_attempts,
            max_operation_elapsed: data.max_operation_elapsed,
            max_total_elapsed: data.max_total_elapsed,
        })?;
        Ok(Self::new(limits, data.backoff))
    }
}
