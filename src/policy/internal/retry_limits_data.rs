// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private serde wire DTO for [`crate::RetryLimits`].

use std::num::NonZeroU32;

use serde::Deserialize;
use serde::Serialize;

use super::DurationData;
use crate::RetryLimits;
use crate::RetryPolicyError;

/// Unvalidated limits data accepted only through checked conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetryLimitsData {
    /// Maximum number of attempts including the first attempt.
    pub(crate) max_attempts: u32,
    /// Optional cumulative operation budget.
    pub(crate) max_operation_elapsed: Option<DurationData>,
    /// Optional whole-flow budget.
    pub(crate) max_total_elapsed: Option<DurationData>,
}

impl From<&RetryLimits> for RetryLimitsData {
    /// Copies validated runtime limits to their stable wire representation.
    fn from(limits: &RetryLimits) -> Self {
        Self {
            max_attempts: limits.max_attempts().get(),
            max_operation_elapsed: limits
                .max_operation_elapsed()
                .map(DurationData::from),
            max_total_elapsed: limits
                .max_total_elapsed()
                .map(DurationData::from),
        }
    }
}

impl TryFrom<RetryLimitsData> for RetryLimits {
    type Error = RetryPolicyError;

    /// Converts wire limits after validating nonzero attempts and durations.
    fn try_from(data: RetryLimitsData) -> Result<Self, Self::Error> {
        let max_attempts =
            NonZeroU32::new(data.max_attempts).ok_or_else(|| {
                RetryPolicyError::new(
                    "max_attempts",
                    "maximum attempts must be greater than zero",
                )
            })?;
        Ok(Self::new(
            max_attempts,
            data.max_operation_elapsed
                .map(TryInto::try_into)
                .transpose()?,
            data.max_total_elapsed.map(TryInto::try_into).transpose()?,
        ))
    }
}
