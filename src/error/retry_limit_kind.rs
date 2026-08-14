// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry continuation limit classifications.

use std::fmt;

/// Limit that prevented another retry attempt from starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryLimitKind {
    /// The configured attempt count was exhausted.
    Attempts,
    /// The cumulative operation-time budget was exhausted.
    OperationElapsed,
    /// The total retry-flow elapsed-time budget was exhausted.
    TotalElapsed,
}

impl fmt::Display for RetryLimitKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Attempts => "attempts",
            Self::OperationElapsed => "operation elapsed",
            Self::TotalElapsed => "total elapsed",
        };
        formatter.write_str(name)
    }
}
