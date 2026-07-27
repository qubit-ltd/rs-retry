// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry-after hint merge policy.

use std::fmt;
use std::str::FromStr;

/// Controls how a retry-after hint interacts with the configured delay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RetryAfterPolicy {
    /// Use the hint as the retry delay.
    #[default]
    Replace,
    /// Use the longer of the hint and configured retry delay.
    AtLeastConfiguredDelay,
}

impl fmt::Display for RetryAfterPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Replace => "replace",
            Self::AtLeastConfiguredDelay => "at_least_configured_delay",
        })
    }
}

impl FromStr for RetryAfterPolicy {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "at_least_configured_delay" => Ok(Self::AtLeastConfiguredDelay),
            _ => Err(
                "retry_after_policy must be `replace` or `at_least_configured_delay`".to_owned(),
            ),
        }
    }
}
