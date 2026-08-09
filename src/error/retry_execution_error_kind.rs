// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stable retry infrastructure failure categories.

use serde::Deserialize;
use serde::Serialize;

/// Infrastructure component that failed after or around an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryExecutionErrorKind {
    /// The configured timer or sleeper failed.
    Timer,
    /// A worker could not be safely reaped.
    Worker,
}

impl std::fmt::Display for RetryExecutionErrorKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Timer => "timer",
            Self::Worker => "worker",
        };
        formatter.write_str(name)
    }
}
