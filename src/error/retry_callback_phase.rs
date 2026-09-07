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
    /// A before-attempt observer was running.
    BeforeAttempt,
    /// An attempt-failed observer was running.
    AttemptFailed,
    /// A retry rule was deciding whether to continue.
    RuleDecision,
    /// A retry-scheduled observer was running.
    RetryScheduled,
    /// A successful-flow observer was running.
    Success,
    /// A terminal-failure observer was running.
    TerminalFailure,
}

impl fmt::Display for RetryCallbackPhase {
    ///
    /// Formats the stable human-readable label of this classification.
    ///
    /// # Parameters
    /// - `formatter`: Destination supplied by the formatting machinery.
    ///
    /// # Returns
    /// The result of writing this diagnostic representation.
    ///
    /// # Errors
    /// Returns a formatting error if the destination rejects a write.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::BeforeAttempt => "before attempt",
            Self::AttemptFailed => "attempt failed",
            Self::RuleDecision => "rule decision",
            Self::RetryScheduled => "retry scheduled",
            Self::Success => "success",
            Self::TerminalFailure => "terminal failure",
        };
        formatter.write_str(name)
    }
}
