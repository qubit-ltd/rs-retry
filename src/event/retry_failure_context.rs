/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Failure context payload.
//!
//! Failure contexts are emitted when retry limits stop the operation without a
//! successful result.

use std::time::Duration;

/// Context emitted when retry limits are exhausted.
///
/// Carries failure metadata while the final failure payload is passed
/// separately to failure listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryFailureContext {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
}
