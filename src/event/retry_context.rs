/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry context payload.
//!
//! Retry contexts are emitted after an attempt fails and before the executor sleeps
//! for the next attempt.

use std::time::Duration;

/// Context emitted before a retry sleep.
///
/// Carries retry metadata while the triggering failure is passed separately to
/// retry listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryContext {
    /// Attempt that just failed.
    pub attempt: u32,
    /// Configured maximum attempts.
    pub max_attempts: u32,
    /// Elapsed time observed before sleeping.
    pub elapsed: Duration,
    /// RetryDelay that will be slept before the next attempt.
    pub next_delay: Duration,
}
