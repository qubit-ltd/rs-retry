// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Outcome of waiting for a blocking retry delay.

use qubit_clock::TimeError;

/// Result of waiting for one blocking retry delay.
pub(crate) enum BlockingBackoffOutcome {
    /// The configured delay elapsed.
    Elapsed,
    /// Flow cancellation interrupted the delay.
    Cancelled,
    /// Registering or polling the delay timer failed.
    TimerFailed(
        /// Error from registering or polling the backoff timer.
        TimeError,
    ),
}
