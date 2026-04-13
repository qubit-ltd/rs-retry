/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Attempt context passed to classifiers.
//!
//! The context carries executor state that helps a classifier decide whether an
//! application error should be retried.

use std::time::Duration;

/// Context visible to error classifiers.
///
/// Values are snapshots taken before the classifier is invoked for a failed
/// attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptContext {
    /// Current attempt number, starting at 1.
    pub attempt: u32,
    /// Configured maximum attempts.
    pub max_attempts: u32,
    /// Elapsed time observed before the classifier is invoked.
    pub elapsed: Duration,
}
