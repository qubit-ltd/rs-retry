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
}

impl fmt::Display for WorkerStopTrigger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::AttemptTimeout => "attempt timeout",
            Self::FlowTimeout => "flow timeout",
            Self::Cancellation => "cancellation",
        };
        formatter.write_str(name)
    }
}
