/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Success context payload.
//!
//! Success contexts are emitted once a retry executor receives an `Ok` result from
//! the operation.

use std::time::Duration;

/// Context emitted when an operation succeeds.
///
/// The context contains execution metadata only; it does not borrow or clone the
/// successful result value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySuccessContext {
    /// Number of attempts that were executed.
    pub attempts: u32,
    /// Total elapsed time observed by the retry executor.
    pub elapsed: Duration,
}
