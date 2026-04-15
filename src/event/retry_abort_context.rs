/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Abort context payload.
//!
//! Abort contexts are emitted when the error classifier chooses not to retry an
//! application error.

use std::time::Duration;

/// Context emitted when the classifier aborts the operation.
///
/// Carries abort metadata while the triggering failure is passed separately to
/// abort listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAbortContext {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
}
