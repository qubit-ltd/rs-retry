// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable serde representation for [`std::time::Duration`].

use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

use crate::RetryPolicyError;

/// Wire duration with independently validated second and nanosecond fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DurationData {
    /// Whole seconds.
    pub(crate) seconds: u64,
    /// Fractional nanoseconds, strictly below one second.
    pub(crate) nanoseconds: u32,
}

impl From<Duration> for DurationData {
    /// Converts a runtime duration to its fixed wire representation.
    ///
    /// # Parameters
    /// - `duration`: Validated runtime value to represent on the wire.
    ///
    /// # Returns
    /// A wire representation with normalized seconds and nanoseconds.
    #[inline]
    fn from(duration: Duration) -> Self {
        Self {
            seconds: duration.as_secs(),
            nanoseconds: duration.subsec_nanos(),
        }
    }
}

impl TryFrom<DurationData> for Duration {
    /// Invalid policy input or encoded duration.
    type Error = RetryPolicyError;

    /// Converts wire data after validating nanoseconds and checked addition.
    ///
    /// # Parameters
    /// - `data`: Unvalidated wire value consumed by conversion.
    ///
    /// # Returns
    /// The validated runtime value without borrowing the wire input.
    ///
    /// # Errors
    /// Rejects nanoseconds at least one billion or unrepresentable duration.
    fn try_from(data: DurationData) -> Result<Self, Self::Error> {
        if data.nanoseconds >= 1_000_000_000 {
            return Err(RetryPolicyError::new(
                "duration.nanoseconds",
                "nanoseconds must be less than 1_000_000_000",
            ));
        }
        Duration::from_secs(data.seconds)
            .checked_add(Duration::from_nanos(u64::from(data.nanoseconds)))
            .ok_or_else(|| RetryPolicyError::new("duration", "duration exceeds the supported range"))
    }
}
