// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Result returned from one blocking worker attempt.

use crate::AttemptFailure;
use crate::WorkerStopTrigger;

/// Outcome of starting and waiting for one blocking worker attempt.
pub(in crate::executor) enum BlockingAttemptOutcome<T, E> {
    /// The worker completed and returned its operation result.
    Completed(Result<T, AttemptFailure<E>>),
    /// The operating-system worker thread could not be started.
    WorkerSpawnFailed {
        /// Diagnostic supplied by the thread runtime.
        message: Box<str>,
    },
    /// A stop event won and the worker exited during the grace period.
    Stopped {
        /// First event that requested the worker to stop.
        trigger: WorkerStopTrigger,
    },
    /// A stop event won but the worker did not exit during the grace period.
    WorkerStillRunning {
        /// First event that requested the worker to stop.
        trigger: WorkerStopTrigger,
    },
}
