// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private serde wire DTO for [`crate::RetryAdmissionLimits`].

use std::num::NonZeroU32;

use serde::Deserialize;
use serde::Serialize;

use super::DurationData;
use crate::RetryAdmissionLimits;
use crate::RetryPolicyError;

/// Unvalidated limits data accepted only through checked conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetryAdmissionLimitsData {
    /// Maximum number of attempts including the first attempt.
    pub(crate) max_attempts: u32,
    /// Optional cumulative operation budget.
    pub(crate) operation_time_budget: Option<DurationData>,
    /// Optional whole-flow budget.
    pub(crate) total_time_budget: Option<DurationData>,
}

impl From<&RetryAdmissionLimits> for RetryAdmissionLimitsData {
    /// Copies validated runtime limits to their stable wire representation.
    ///
    /// # Parameters
    /// - `limits`: Validated runtime value to represent on the wire.
    ///
    /// # Returns
    /// Wire limits preserving optional elapsed budgets.
    #[inline]
    fn from(limits: &RetryAdmissionLimits) -> Self {
        Self {
            max_attempts: limits.max_attempts().get(),
            operation_time_budget: limits.operation_time_budget().map(DurationData::from),
            total_time_budget: limits.total_time_budget().map(DurationData::from),
        }
    }
}

impl TryFrom<RetryAdmissionLimitsData> for RetryAdmissionLimits {
    /// Invalid policy input or encoded duration.
    type Error = RetryPolicyError;

    /// Converts wire limits after validating nonzero attempts and durations.
    ///
    /// # Parameters
    /// - `data`: Unvalidated wire value consumed by conversion.
    ///
    /// # Returns
    /// The validated runtime value without borrowing the wire input.
    ///
    /// # Errors
    /// Rejects zero attempts and invalid encoded durations.
    fn try_from(data: RetryAdmissionLimitsData) -> Result<Self, Self::Error> {
        let max_attempts = NonZeroU32::new(data.max_attempts)
            .ok_or_else(|| RetryPolicyError::new("max_attempts", "maximum attempts must be greater than zero"))?;
        Ok(Self::new(
            max_attempts,
            data.operation_time_budget.map(TryInto::try_into).transpose()?,
            data.total_time_budget.map(TryInto::try_into).transpose()?,
        ))
    }
}
