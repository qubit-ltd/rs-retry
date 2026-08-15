// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry callback phase classifications.

use std::fmt;

/// Lifecycle phase in which a retry callback failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryCallbackPhase {
    /// An attempt-started observer was running.
    AttemptStarted,
    /// An attempt-failed observer was running.
    AttemptFailed,
    /// A retry rule was deciding whether to continue.
    RuleDecision,
    /// A retry-scheduled observer was running.
    RetryScheduled,
}

impl fmt::Display for RetryCallbackPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::AttemptStarted => "attempt started",
            Self::AttemptFailed => "attempt failed",
            Self::RuleDecision => "rule decision",
            Self::RetryScheduled => "retry scheduled",
        };
        formatter.write_str(name)
    }
}
