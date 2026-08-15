// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry timeout scope classifications.

use std::fmt;

/// Scope whose hard timeout stopped retry execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryTimeoutScope {
    /// The timeout applied to one admitted attempt.
    Attempt,
    /// The timeout applied to the complete retry flow.
    Flow,
}

impl fmt::Display for RetryTimeoutScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Attempt => "attempt",
            Self::Flow => "flow",
        };
        formatter.write_str(name)
    }
}
