// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry continuation budgets.

use std::num::NonZeroU32;
use std::time::Duration;

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Deserializer;
#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "serde")]
use serde::Serializer;

#[cfg(feature = "serde")]
use super::internal::RetryLimitsData;
/// Limits that decide whether a retry flow may continue.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryLimits {
    max_attempts: NonZeroU32,
    max_operation_elapsed: Option<Duration>,
    max_total_elapsed: Option<Duration>,
}

#[cfg(feature = "serde")]
impl Serialize for RetryLimits {
    /// Serializes validated limits through the stable private wire DTO.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RetryLimitsData::from(self).serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for RetryLimits {
    /// Deserializes limits and rejects invalid or unknown configuration data.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = RetryLimitsData::deserialize(deserializer)?;
        Self::try_from(data).map_err(serde::de::Error::custom)
    }
}

impl RetryLimits {
    /// Creates validated retry limits.
    pub(crate) fn new(
        max_attempts: NonZeroU32,
        max_operation_elapsed: Option<Duration>,
        max_total_elapsed: Option<Duration>,
    ) -> Self {
        Self {
            max_attempts,
            max_operation_elapsed,
            max_total_elapsed,
        }
    }

    /// Returns the maximum number of attempts, including the first attempt.
    #[must_use]
    pub fn max_attempts(&self) -> NonZeroU32 {
        self.max_attempts
    }

    /// Returns the cumulative operation-time budget.
    #[must_use]
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns the whole-flow wall-clock budget.
    #[must_use]
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }
}
