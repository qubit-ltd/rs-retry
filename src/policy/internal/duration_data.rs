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
    fn from(duration: Duration) -> Self {
        Self {
            seconds: duration.as_secs(),
            nanoseconds: duration.subsec_nanos(),
        }
    }
}

impl TryFrom<DurationData> for Duration {
    type Error = RetryPolicyError;

    /// Converts wire data after validating nanoseconds and checked addition.
    fn try_from(data: DurationData) -> Result<Self, Self::Error> {
        if data.nanoseconds >= 1_000_000_000 {
            return Err(RetryPolicyError::new(
                "duration.nanoseconds",
                "nanoseconds must be less than 1_000_000_000",
            ));
        }
        Duration::from_secs(data.seconds)
            .checked_add(Duration::from_nanos(u64::from(data.nanoseconds)))
            .ok_or_else(|| {
                RetryPolicyError::new(
                    "duration",
                    "duration exceeds the supported range",
                )
            })
    }
}
