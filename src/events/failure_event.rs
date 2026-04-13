/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Failure event payload.
//!
//! Failure events are emitted when retry limits stop the operation without a
//! successful result.

use std::time::Duration;

use crate::AttemptFailure;

/// Event emitted when retry limits are exhausted.
///
/// The final failure is optional because a zero elapsed-time budget can stop a
/// executor before the first attempt runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureEvent<'a, E> {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
    /// Borrowed final failure when one exists.
    pub failure: Option<&'a AttemptFailure<E>>,
}
