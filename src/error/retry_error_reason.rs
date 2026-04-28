/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Terminal retry-flow error reasons.

use serde::{Deserialize, Serialize};

/// Reason why the whole retry flow stopped with an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryErrorReason {
    /// A listener or retry policy aborted the retry flow.
    Aborted,
    /// No attempts remain.
    AttemptsExceeded,
    /// The cumulative user operation elapsed-time budget was exhausted.
    MaxOperationElapsedExceeded,
    /// The total monotonic retry-flow elapsed-time budget was exhausted.
    MaxTotalElapsedExceeded,
    /// The operation mode does not support the configured behavior.
    ///
    /// Currently used when [`Retry::run`](crate::Retry::run) receives
    /// configured per-attempt timeout options.
    UnsupportedOperation,
    /// A timed-out blocking worker did not exit within the cancellation grace period.
    WorkerStillRunning,
}
