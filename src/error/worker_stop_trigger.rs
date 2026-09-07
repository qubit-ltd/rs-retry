// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Worker stop trigger classifications.

use std::fmt;

/// Event that requested a worker attempt to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStopTrigger {
    /// The current attempt timeout expired.
    AttemptTimeout,
    /// The whole-flow timeout expired.
    FlowTimeout,
    /// The retry flow was externally cancelled.
    Cancellation,
    /// The active attempt timer failed.
    TimerFailure,
}

impl fmt::Display for WorkerStopTrigger {
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
            Self::AttemptTimeout => "attempt timeout",
            Self::FlowTimeout => "flow timeout",
            Self::Cancellation => "cancellation",
            Self::TimerFailure => "timer failure",
        };
        formatter.write_str(name)
    }
}
