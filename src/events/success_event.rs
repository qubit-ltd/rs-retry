/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Success event payload.
//!
//! Success events are emitted once a retry executor receives an `Ok` result from
//! the operation.

use std::time::Duration;

/// Event emitted when an operation succeeds.
///
/// The event contains execution metadata only; it does not borrow or clone the
/// successful result value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuccessEvent {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
}
