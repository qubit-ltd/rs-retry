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
use crate::RetryAdmissionLimits;
use crate::RetryPolicy;
use crate::RetryPolicyError;

/// Unvalidated policy data with the stable public wire-field layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetryPolicyData {
    /// Maximum number of attempts including the first attempt.
    pub(crate) max_attempts: u32,
    /// Optional cumulative operation budget.
    pub(crate) operation_time_budget: Option<DurationData>,
    /// Optional whole-flow budget.
    pub(crate) total_time_budget: Option<DurationData>,
    /// Validated backoff strategy data.
    pub(crate) backoff: BackoffPolicy,
}

impl From<&RetryPolicy> for RetryPolicyData {
    /// Copies a policy into its stable public wire-field layout.
    ///
    /// # Parameters
    /// - `policy`: Validated runtime value to represent on the wire.
    ///
    /// # Returns
    /// Wire policy preserving validated backoff and continuation budgets.
    #[inline]
    fn from(policy: &RetryPolicy) -> Self {
        let limits = policy.admission_limits();
        Self {
            max_attempts: limits.max_attempts().get(),
            operation_time_budget: limits
                .operation_time_budget()
                .map(DurationData::from),
            total_time_budget: limits
                .total_time_budget()
                .map(DurationData::from),
            backoff: policy.backoff().clone(),
        }
    }
}

impl TryFrom<RetryPolicyData> for RetryPolicy {
    /// Invalid policy input or encoded duration.
    type Error = RetryPolicyError;

    /// Converts wire data through the same limits validation as the builder.
    ///
    /// # Parameters
    /// - `data`: Unvalidated wire value consumed by conversion.
    ///
    /// # Returns
    /// The validated runtime value without borrowing the wire input.
    ///
    /// # Errors
    /// Rejects zero attempts and invalid encoded elapsed durations.
    fn try_from(data: RetryPolicyData) -> Result<Self, Self::Error> {
        let limits =
            RetryAdmissionLimits::try_from(super::RetryAdmissionLimitsData {
                max_attempts: data.max_attempts,
                operation_time_budget: data.operation_time_budget,
                total_time_budget: data.total_time_budget,
            })?;
        Ok(Self::new(limits, data.backoff))
    }
}
