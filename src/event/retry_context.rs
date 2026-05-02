/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry event context payload.
//!
//! A retry context is the shared metadata snapshot passed to attempt, failure,
//! and terminal-error listeners.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{AttemptTimeoutSource, RetryContextParts};

/// Context emitted for retry lifecycle events.
///
/// `attempt` is one-based for attempt-related events and zero when a retry flow
/// stops before any attempt is executed. `operation_elapsed` is cumulative user
/// operation execution time only; listener and retry-sleep time are excluded.
/// `total_elapsed` is monotonic elapsed time spent in the retry flow and
/// includes operation execution, retry sleep, retry-after sleep, and
/// retry-control listener time. `attempt_elapsed` is set after an attempt
/// completes and is zero before an attempt starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryContext {
    /// Current attempt number, or zero if no attempt has run.
    attempt: u32,
    /// Configured maximum attempts.
    max_attempts: u32,
    /// Configured maximum cumulative user operation time.
    max_operation_elapsed: Option<Duration>,
    /// Configured maximum total retry-flow elapsed time.
    max_total_elapsed: Option<Duration>,
    /// Cumulative user operation time consumed by this retry flow.
    operation_elapsed: Duration,
    /// Total monotonic time consumed by this retry flow.
    total_elapsed: Duration,
    /// Elapsed time spent in the current attempt.
    attempt_elapsed: Duration,
    /// Effective timeout configured for the current attempt.
    attempt_timeout: Option<Duration>,
    /// Delay selected before the next attempt, when known.
    next_delay: Option<Duration>,
    /// Optional retry-after hint extracted before failure policy runs.
    retry_after_hint: Option<Duration>,
    /// Source used for the last selected per-attempt timeout.
    attempt_timeout_source: Option<AttemptTimeoutSource>,
    /// Worker attempts that timed out and were not observed to exit before the
    /// cancellation grace period ended.
    unreaped_worker_count: u32,
}

impl RetryContext {
    /// Creates a public retry context snapshot with default timing metadata.
    ///
    /// # Parameters
    /// - `attempt`: Current attempt number, starting at 1, or 0 before any
    ///   attempt has run.
    /// - `max_attempts`: Configured maximum attempts.
    ///
    /// # Returns
    /// A retry context with no elapsed budgets, elapsed values, selected next
    /// delay, retry-after hint, or attempt timeout.
    pub fn new(attempt: u32, max_attempts: u32) -> Self {
        Self::from_parts(RetryContextParts {
            attempt,
            max_attempts,
            max_operation_elapsed: None,
            max_total_elapsed: None,
            operation_elapsed: Duration::ZERO,
            total_elapsed: Duration::ZERO,
            attempt_elapsed: Duration::ZERO,
            attempt_timeout: None,
        })
    }

    /// Creates a retry context snapshot from internal parts.
    ///
    /// # Parameters
    /// - `parts`: Internal context payload.
    ///
    /// # Returns
    /// A retry context with no selected next delay or retry-after hint.
    pub(crate) fn from_parts(parts: RetryContextParts) -> Self {
        Self {
            attempt: parts.attempt,
            max_attempts: parts.max_attempts,
            max_operation_elapsed: parts.max_operation_elapsed,
            max_total_elapsed: parts.max_total_elapsed,
            operation_elapsed: parts.operation_elapsed,
            total_elapsed: parts.total_elapsed,
            attempt_elapsed: parts.attempt_elapsed,
            attempt_timeout: parts.attempt_timeout,
            next_delay: None,
            retry_after_hint: None,
            attempt_timeout_source: None,
            unreaped_worker_count: 0,
        }
    }

    /// Returns this event's attempt number.
    ///
    /// # Returns
    /// A one-based attempt number, or zero if no attempt has run.
    #[inline]
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns the maximum number of attempts.
    ///
    /// # Returns
    /// The configured maximum attempts, including the initial attempt.
    #[inline]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Returns the maximum number of retries.
    ///
    /// # Returns
    /// The configured maximum retry count after the initial attempt.
    #[inline]
    pub fn max_retries(&self) -> u32 {
        self.max_attempts.saturating_sub(1)
    }

    /// Returns the optional cumulative user operation time budget.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded retry flows, or `None` for unlimited flows.
    #[inline]
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns the optional total retry-flow elapsed time budget.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded retry flows, or `None` for unlimited flows.
    #[inline]
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }

    /// Returns cumulative user operation time consumed by the retry flow.
    ///
    /// # Returns
    /// Total user operation time observed at this event. Listener execution and
    /// retry sleeps are excluded.
    #[inline]
    pub fn operation_elapsed(&self) -> Duration {
        self.operation_elapsed
    }

    /// Returns total monotonic time consumed by the retry flow.
    ///
    /// # Returns
    /// Total retry-flow time observed at this event. Operation execution, retry
    /// sleep, retry-after sleep, and retry-control listener time are included.
    #[inline]
    pub fn total_elapsed(&self) -> Duration {
        self.total_elapsed
    }

    /// Returns elapsed time spent in the current attempt.
    ///
    /// # Returns
    /// Attempt elapsed time. Before-attempt events report zero.
    #[inline]
    pub fn attempt_elapsed(&self) -> Duration {
        self.attempt_elapsed
    }

    /// Returns the effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// `Some(Duration)` when this attempt is bounded by configured timeout, by
    /// the remaining max-operation-elapsed budget, or by the remaining
    /// max-total-elapsed budget.
    #[inline]
    pub fn attempt_timeout(&self) -> Option<Duration> {
        self.attempt_timeout
    }

    /// Returns the effective source of the current attempt timeout.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(AttemptTimeoutSource::Configured)` when the current attempt timeout
    /// came from configured attempt timeout options, `Some(AttemptTimeoutSource::MaxOperationElapsed)`
    /// when it came from remaining max-operation-elapsed budget,
    /// `Some(AttemptTimeoutSource::MaxTotalElapsed)` when it came from remaining
    /// max-total-elapsed budget, otherwise `None`.
    #[inline]
    pub fn attempt_timeout_source(&self) -> Option<AttemptTimeoutSource> {
        self.attempt_timeout_source
    }

    /// Returns the number of worker attempts not observed to exit after cancellation.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Count of timed-out worker attempts whose worker thread did not finish
    /// before the cancellation grace period ended. With the current fail-closed
    /// worker policy this is either `0` or `1` for a single retry flow.
    #[inline]
    pub fn unreaped_worker_count(&self) -> u32 {
        self.unreaped_worker_count
    }

    /// Returns the delay selected before the next attempt.
    ///
    /// # Returns
    /// `Some(Duration)` in retry-scheduled events after a next delay has been
    /// selected; otherwise `None`.
    #[inline]
    pub fn next_delay(&self) -> Option<Duration> {
        self.next_delay
    }

    /// Returns a retry-after hint extracted from the failure.
    ///
    /// # Returns
    /// `Some(Duration)` when a configured hint extractor produced a value.
    #[inline]
    pub fn retry_after_hint(&self) -> Option<Duration> {
        self.retry_after_hint
    }

    /// Returns a copy of this context with a selected retry delay.
    ///
    /// # Parameters
    /// - `delay`: Delay selected before the next attempt.
    ///
    /// # Returns
    /// A context carrying the selected delay.
    #[inline]
    pub(crate) fn with_next_delay(mut self, delay: Duration) -> Self {
        self.next_delay = Some(delay);
        self
    }

    /// Returns a copy of this context with refreshed total elapsed time.
    ///
    /// # Parameters
    /// - `total_elapsed`: Total monotonic time consumed by the retry flow.
    ///
    /// # Returns
    /// A context carrying the refreshed total elapsed value.
    #[inline]
    pub(crate) fn with_total_elapsed(mut self, total_elapsed: Duration) -> Self {
        self.total_elapsed = total_elapsed;
        self
    }

    /// Returns a copy of this context with a retry-after hint.
    ///
    /// # Parameters
    /// - `hint`: Optional retry-after hint.
    ///
    /// # Returns
    /// A context carrying the hint.
    #[inline]
    pub(crate) fn with_retry_after_hint(mut self, hint: Option<Duration>) -> Self {
        self.retry_after_hint = hint;
        self
    }

    /// Returns a copy of this context with a timeout source.
    ///
    /// # Parameters
    /// - `source`: Source of the current attempt timeout, if any.
    ///
    /// # Returns
    /// A context carrying the timeout source when available.
    #[inline]
    pub(crate) fn with_attempt_timeout_source(
        mut self,
        source: Option<AttemptTimeoutSource>,
    ) -> Self {
        if let Some(source) = source {
            self.attempt_timeout_source = Some(source);
        }
        self
    }

    /// Returns a copy of this context with unreaped worker count.
    ///
    /// # Parameters
    /// - `count`: Number of worker attempts not observed to exit after cancellation.
    ///
    /// # Returns
    /// A context carrying the worker cleanup metric.
    #[inline]
    pub(crate) fn with_unreaped_worker_count(mut self, count: u32) -> Self {
        self.unreaped_worker_count = count;
        self
    }
}
