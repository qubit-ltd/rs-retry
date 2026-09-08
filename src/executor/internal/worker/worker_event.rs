// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Events exchanged by the worker, reaper, and waiting thread.

use crate::AttemptFailure;
use crate::RetryPanic;

/// Event observed while waiting for one worker attempt.
/// # Type Parameters
/// - `E`: Application error transferred from worker to waiting thread.
pub(super) enum WorkerEvent<E> {
    /// The operation returned; thread-local destruction may still be running.
    Completed(
        /// Owned operation result; does not prove thread-local cleanup.
        Result<(), AttemptFailure<E>>,
    ),
    /// The reaper joined the worker, including thread-local destruction.
    Joined(
        /// Join success or decoded unwind payload from thread exit.
        Result<(), RetryPanic>,
    ),
    /// A timer or cancellation future needs another poll.
    Wake,
}
