/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Mutable state shared by retry execution loops.

use std::time::{
    Duration,
    Instant,
};

use crate::AttemptFailure;

/// Mutable retry-flow state shared by sync, async, and worker execution loops.
pub(in crate::executor) struct RetryFlowState<E> {
    /// Monotonic instant when the retry flow started.
    pub(in crate::executor) started_at: Instant,
    /// Cumulative user operation time consumed by attempts.
    pub(in crate::executor) operation_elapsed: Duration,
    /// Attempts executed or currently being prepared.
    pub(in crate::executor) attempts: u32,
    /// Last failure retained for elapsed-budget errors raised before another attempt.
    last_failure: Option<AttemptFailure<E>>,
}

impl<E> RetryFlowState<E> {
    /// Creates an empty retry-flow state.
    ///
    /// # Returns
    /// A state with zero attempts and no accumulated operation elapsed time.
    pub(in crate::executor) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            operation_elapsed: Duration::ZERO,
            attempts: 0,
            last_failure: None,
        }
    }

    /// Returns total monotonic retry-flow elapsed time.
    ///
    /// # Returns
    /// Elapsed time since this retry flow started.
    #[inline]
    pub(in crate::executor) fn total_elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Marks the next attempt as started.
    ///
    /// # Returns
    /// The one-based attempt number after incrementing.
    #[inline]
    pub(in crate::executor) fn start_next_attempt(&mut self) -> u32 {
        self.attempts += 1;
        self.attempts
    }

    /// Adds elapsed user operation time.
    ///
    /// # Parameters
    /// - `attempt_elapsed`: Duration consumed by the latest attempt.
    #[inline]
    pub(in crate::executor) fn add_operation_elapsed(&mut self, attempt_elapsed: Duration) {
        self.operation_elapsed = self.operation_elapsed.saturating_add(attempt_elapsed);
    }

    /// Stores the last failure observed before a retry sleep.
    ///
    /// # Parameters
    /// - `failure`: Failure from the latest attempt.
    #[inline]
    pub(in crate::executor) fn record_last_failure(&mut self, failure: AttemptFailure<E>) {
        self.last_failure = Some(failure);
    }

    /// Takes the retained last failure.
    ///
    /// # Returns
    /// The retained last failure, if one exists.
    #[inline]
    pub(in crate::executor) fn take_last_failure(&mut self) -> Option<AttemptFailure<E>> {
        self.last_failure.take()
    }
}
