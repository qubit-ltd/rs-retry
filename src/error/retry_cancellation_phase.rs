// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry cancellation phase classifications.

use std::fmt;

/// Retry phase in which cancellation stopped the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryCancellationPhase {
    /// Cancellation was observed before an operation started.
    BeforeAttempt,
    /// Cancellation interrupted an admitted attempt.
    Attempt,
    /// Cancellation interrupted the delay before another attempt.
    Backoff,
}

impl fmt::Display for RetryCancellationPhase {
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
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::BeforeAttempt => "before attempt",
            Self::Attempt => "attempt",
            Self::Backoff => "backoff",
        };
        formatter.write_str(name)
    }
}
