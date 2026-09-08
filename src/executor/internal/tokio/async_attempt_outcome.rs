// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! AsyncAttemptOutcome state shared by asynchronous executors.

use qubit_clock::TimeError;

use crate::RetryTimeoutScope;

/// Result of running an async attempt and its cooperative timeout timer.
/// # Type Parameters
/// - `T`: Successful value owned by the completed operation.
/// - `E`: Application error owned by the completed operation.
pub(in crate::executor) enum AsyncAttemptOutcome<T, E> {
    /// The operation future completed before its timeout.
    Completed(
        /// Owned result selected before cancellation or timeout.
        Result<T, E>,
    ),
    /// The cooperative timeout completed first.
    TimedOut(
        /// Boundary whose registered timer completed.
        RetryTimeoutScope,
    ),
    /// Cooperative cancellation interrupted the operation future.
    Cancelled,
    /// Registering or polling the timeout timer failed.
    TimerFailed(
        /// Error from registering or polling the attempt timer.
        TimeError,
    ),
}
