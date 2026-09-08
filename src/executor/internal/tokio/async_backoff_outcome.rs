// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! AsyncBackoffOutcome state used by the Tokio executor.

use qubit_clock::TimeError;

/// Result of waiting for one retry delay.
pub(in crate::executor) enum AsyncBackoffOutcome {
    /// The delay elapsed normally.
    Elapsed,
    /// Cooperative cancellation interrupted the delay.
    Cancelled,
    /// Registering or polling the delay timer failed.
    TimerFailed(
        /// Error from registering or polling the backoff timer.
        TimeError,
    ),
}
