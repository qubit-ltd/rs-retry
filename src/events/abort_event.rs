/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Abort event payload.
//!
//! Abort events are emitted when the error classifier chooses not to retry an
//! application error.

use std::time::Duration;

use crate::AttemptFailure;

/// Event emitted when the classifier aborts the operation.
///
/// The event borrows the failure so listeners can inspect it without forcing
/// the executor to clone or move the caller's error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbortEvent<'a, E> {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
    /// Borrowed failure that caused the abort.
    pub failure: &'a AttemptFailure<E>,
}
