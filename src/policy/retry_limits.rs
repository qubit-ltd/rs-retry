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
use serde::de::Error;

#[cfg(feature = "serde")]
use super::internal::RetryLimitsData;
/// Limits that decide whether a retry flow may continue.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryLimits {
    /// Maximum admissions including the initial operation.
    max_attempts: NonZeroU32,
    /// Optional cumulative operation-time limit; None disables this soft
    /// budget.
    max_operation_elapsed: Option<Duration>,
    /// Optional monotonic whole-flow limit; None disables this soft budget.
    max_total_elapsed: Option<Duration>,
}

#[cfg(feature = "serde")]
impl Serialize for RetryLimits {
    /// Serializes validated limits through the stable private wire DTO.
    ///
    /// # Type Parameters
    /// - `S`: Serializer for the stable configuration representation.
    ///
    /// # Parameters
    /// - `serializer`: Serialization destination, consumed by this call.
    ///
    /// # Returns
    /// The serializer output for the validated configuration.
    ///
    /// # Errors
    /// Returns any serialization error reported by the destination.
    #[inline(always)]
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
    ///
    /// # Type Parameters
    /// - `D`: Deserializer borrowing input for its declared lifetime.
    ///
    /// # Parameters
    /// - `deserializer`: Source of the stable wire configuration.
    ///
    /// # Returns
    /// A validated configuration value.
    ///
    /// # Errors
    /// Rejects malformed input, unknown fields and invalid policy/duration
    /// values.
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = RetryLimitsData::deserialize(deserializer)?;
        Self::try_from(data).map_err(Error::custom)
    }
}

impl RetryLimits {
    /// Creates validated retry limits.
    ///
    /// # Parameters
    /// - `max_attempts`: Nonzero admission limit, including the initial
    ///   operation.
    /// - `max_operation_elapsed`: Some cumulative operation budget, or None to
    ///   disable.
    /// - `max_total_elapsed`: Some whole-flow budget, or None to disable.
    ///
    /// # Returns
    /// Immutable continuation limits; zero elapsed budgets prohibit admission.
    #[inline]
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
    ///
    /// # Returns
    /// Nonzero maximum count including the first operation.
    #[must_use]
    #[inline(always)]
    pub fn max_attempts(&self) -> NonZeroU32 {
        self.max_attempts
    }

    /// Returns the cumulative operation-time budget.
    ///
    /// # Returns
    /// Some cumulative operation duration limit, or None if unbounded.
    /// This budget cannot interrupt an already admitted operation.
    #[must_use]
    #[inline(always)]
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns the whole-flow monotonic elapsed budget.
    ///
    /// # Returns
    /// Some monotonic flow duration limit, or None if unbounded.
    /// The duration includes callbacks and backoff; admitted success is
    /// preserved.
    #[must_use]
    #[inline(always)]
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }
}
