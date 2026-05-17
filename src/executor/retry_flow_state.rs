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
//!
//! The retry flow tracks two clocks. `operation_elapsed` is accumulated only
//! from user operation attempts and backs `max_operation_elapsed`; `total_elapsed`
//! is derived from one monotonic start instant and backs `max_total_elapsed`.
//! The last failure is retained only across retry sleeps so a pre-attempt
//! elapsed-budget stop can still report the failure that led to the sleep.

use std::time::{
    Duration,
    Instant,
};

use crate::event::RetryContextParts;
use crate::options::EffectiveAttemptTimeout;
use crate::{
    AttemptFailure,
    RetryContext,
    RetryError,
    RetryOptions,
};

/// Mutable retry-flow state shared by sync, async, and worker execution loops.
pub(in crate::executor) struct RetryFlowState<E> {
    /// Monotonic instant when the retry flow started.
    started_at: Instant,
    /// Cumulative user operation time consumed by attempts.
    operation_elapsed: Duration,
    /// Attempts executed or currently being prepared.
    attempts: u32,
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

    /// Returns cumulative user operation time consumed by attempts.
    ///
    /// # Returns
    /// Accumulated user operation duration.
    #[inline]
    pub(in crate::executor) fn operation_elapsed(&self) -> Duration {
        self.operation_elapsed
    }

    /// Returns attempts executed or currently being prepared.
    ///
    /// # Returns
    /// One-based attempt count for attempt contexts, or zero before attempts.
    #[inline]
    pub(in crate::executor) fn attempts(&self) -> u32 {
        self.attempts
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

    /// Builds a context snapshot from this retry-flow state.
    ///
    /// # Parameters
    /// - `options`: Retry options that define configured limits.
    /// - `attempt_elapsed`: Elapsed time in the current attempt.
    /// - `attempt_timeout`: Effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// A retry context using this state's latest values.
    pub(in crate::executor) fn context(
        &self,
        options: &RetryOptions,
        attempt_elapsed: Duration,
        attempt_timeout: EffectiveAttemptTimeout,
    ) -> RetryContext {
        RetryContext::from_parts(RetryContextParts {
            attempt: self.attempts,
            max_attempts: options.max_attempts(),
            max_operation_elapsed: options.max_operation_elapsed(),
            max_total_elapsed: options.max_total_elapsed(),
            operation_elapsed: self.operation_elapsed,
            total_elapsed: self.total_elapsed(),
            attempt_elapsed,
            attempt_timeout: attempt_timeout.duration(),
        })
        .with_attempt_timeout_source(attempt_timeout.source())
    }

    /// Takes an elapsed-budget terminal error when a budget is exhausted.
    ///
    /// # Parameters
    /// - `options`: Retry options that define elapsed budgets.
    /// - `attempt_timeout`: Effective timeout visible in the terminal context.
    ///
    /// # Returns
    /// `Some(RetryError)` when the max-operation-elapsed or max-total-elapsed
    /// budget is exhausted; otherwise `None`. The retained last failure is moved
    /// into the error when present.
    pub(in crate::executor) fn take_elapsed_error(
        &mut self,
        options: &RetryOptions,
        attempt_timeout: EffectiveAttemptTimeout,
    ) -> Option<RetryError<E>> {
        let total_elapsed = self.total_elapsed();
        let reason = options.elapsed_error_reason(self.operation_elapsed, total_elapsed)?;
        Some(RetryError::new(
            reason,
            self.take_last_failure(),
            RetryContext::from_parts(RetryContextParts {
                attempt: self.attempts,
                max_attempts: options.max_attempts(),
                max_operation_elapsed: options.max_operation_elapsed(),
                max_total_elapsed: options.max_total_elapsed(),
                operation_elapsed: self.operation_elapsed,
                total_elapsed,
                attempt_elapsed: Duration::ZERO,
                attempt_timeout: attempt_timeout.duration(),
            })
            .with_attempt_timeout_source(attempt_timeout.source()),
        ))
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
