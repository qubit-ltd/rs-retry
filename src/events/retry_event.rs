/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry event payload.
//!
//! Retry events are emitted after an attempt fails and before the executor sleeps
//! for the next attempt.

use std::time::Duration;

use crate::AttemptFailure;

/// Event emitted before a retry sleep.
///
/// The event borrows the triggering failure and carries the already jittered
/// delay that will be used before the next attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryEvent<'a, E> {
    /// Attempt that just failed.
    pub attempt: u32,
    /// Configured maximum attempts.
    pub max_attempts: u32,
    /// Elapsed time observed before sleeping.
    pub elapsed: Duration,
    /// Delay that will be slept before the next attempt.
    pub next_delay: Duration,
    /// Borrowed failure that triggered the retry.
    pub failure: &'a AttemptFailure<E>,
}
