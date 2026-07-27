// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry-after hint merge policy.

/// Controls how a retry-after hint interacts with the configured delay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RetryAfterPolicy {
    /// Use the hint as the retry delay.
    #[default]
    Replace,
    /// Use the longer of the hint and configured retry delay.
    AtLeastConfiguredDelay,
}
