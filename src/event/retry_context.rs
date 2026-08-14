// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry event context payload.
//!
//! A retry context is the shared metadata snapshot passed to attempt, failure,
//! and terminal-error listeners.

use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

use super::RetryContextParts;

/// Context emitted for retry lifecycle events.
///
/// `attempt` is one-based for attempt-related events and zero when a retry flow
/// stops before any attempt is executed. `operation_elapsed` is cumulative user
/// operation execution time only; listener and retry-sleep time are excluded.
/// `total_elapsed` is monotonic elapsed time spent in the retry flow and
/// includes operation execution, retry sleep, retry-after sleep, and
/// retry-control listener time. `attempt_elapsed` is set after an attempt
/// completes and is zero before an attempt starts.
#[must_use]
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
    /// Worker attempts that timed out and were not observed to exit before the
    /// cancellation grace period ended.
    unreaped_worker_count: u32,
}

impl RetryContext {
    /// Creates a public retry context snapshot with default timing metadata.
    ///
    /// # Arguments
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
    /// # Arguments
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
            unreaped_worker_count: 0,
        }
    }

    /// Returns this event's attempt number.
    ///
    /// A `before_attempt` listener sees the upcoming one-based ordinal before
    /// the attempt is admitted into execution. Attempt result listeners and
    /// terminal errors see the committed attempt count, which remains zero if
    /// execution stops during the first pre-attempt checks.
    /// For example, if the first `before_attempt` callback consumes the total
    /// elapsed budget, that callback sees `1`, the operation runs zero times,
    /// and the terminal context reports `0`.
    ///
    /// # Returns
    /// The upcoming or committed attempt number appropriate to this event.
    #[inline(always)]
    #[must_use]
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns the maximum number of attempts.
    ///
    /// # Returns
    /// The configured maximum attempts, including the initial attempt.
    #[inline(always)]
    #[must_use]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Returns the maximum number of retries.
    ///
    /// # Returns
    /// The configured maximum retry count after the initial attempt.
    #[inline(always)]
    #[must_use]
    pub fn max_retries(&self) -> u32 {
        self.max_attempts.saturating_sub(1)
    }

    /// Returns the optional cumulative user operation time budget.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded retry flows, or `None` for unlimited flows.
    #[inline(always)]
    #[must_use]
    pub fn max_operation_elapsed(&self) -> Option<Duration> {
        self.max_operation_elapsed
    }

    /// Returns the optional total retry-flow elapsed time budget.
    ///
    /// # Returns
    /// `Some(Duration)` for bounded retry flows, or `None` for unlimited flows.
    #[inline(always)]
    #[must_use]
    pub fn max_total_elapsed(&self) -> Option<Duration> {
        self.max_total_elapsed
    }

    /// Returns cumulative user operation time consumed by the retry flow.
    ///
    /// # Returns
    /// Total user operation time observed at this event. Listener execution and
    /// retry sleeps are excluded.
    #[inline(always)]
    #[must_use]
    pub fn operation_elapsed(&self) -> Duration {
        self.operation_elapsed
    }

    /// Returns total monotonic time consumed by the retry flow.
    ///
    /// # Returns
    /// Total retry-flow time observed at this event. Operation execution, retry
    /// sleep, retry-after sleep, and retry-control listener time are included.
    #[inline(always)]
    #[must_use]
    pub fn total_elapsed(&self) -> Duration {
        self.total_elapsed
    }

    /// Returns elapsed time spent in the current attempt.
    ///
    /// # Returns
    /// Attempt elapsed time. Before-attempt events report zero.
    #[inline(always)]
    #[must_use]
    pub fn attempt_elapsed(&self) -> Duration {
        self.attempt_elapsed
    }

    /// Returns the effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// `Some(Duration)` when this attempt is bounded by an explicit attempt
    /// timeout or by the remaining hard flow timeout. Operation and total
    /// continuation budgets only decide whether an attempt may start and are
    /// not represented as an attempt timeout.
    #[inline(always)]
    #[must_use]
    pub fn attempt_timeout(&self) -> Option<Duration> {
        self.attempt_timeout
    }

    /// Returns the number of worker attempts not observed to exit after
    /// cancellation.
    ///
    /// # Returns
    /// Count of timed-out worker attempts whose worker thread did not finish
    /// before the cancellation grace period ended. With the current fail-closed
    /// worker policy this is either `0` or `1` for a single retry flow.
    #[inline(always)]
    #[must_use]
    pub fn unreaped_worker_count(&self) -> u32 {
        self.unreaped_worker_count
    }

    /// Returns the delay selected before the next attempt.
    ///
    /// # Returns
    /// `Some(Duration)` in retry-scheduled events after a next delay has been
    /// selected; otherwise `None`.
    #[inline(always)]
    #[must_use]
    pub fn next_delay(&self) -> Option<Duration> {
        self.next_delay
    }

    /// Returns a retry-after hint extracted from the failure.
    ///
    /// # Returns
    /// `Some(Duration)` when a configured hint extractor produced a value.
    #[inline(always)]
    #[must_use]
    pub fn retry_after_hint(&self) -> Option<Duration> {
        self.retry_after_hint
    }

    /// Returns a copy of this context with a selected retry delay.
    ///
    /// # Arguments
    /// - `delay`: Delay selected before the next attempt.
    ///
    /// # Returns
    /// A context carrying the selected delay.
    #[inline(always)]
    pub(crate) fn with_next_delay(mut self, delay: Duration) -> Self {
        self.next_delay = Some(delay);
        self
    }

    /// Returns a copy carrying the hint used to select the next delay.
    #[inline(always)]
    pub(crate) fn with_retry_after_hint(
        mut self,
        hint: Option<Duration>,
    ) -> Self {
        self.retry_after_hint = hint;
        self
    }

    /// Returns a copy carrying the effective timeout for the current attempt.
    #[inline(always)]
    pub(crate) fn with_attempt_timeout(
        mut self,
        timeout: Option<Duration>,
    ) -> Self {
        self.attempt_timeout = timeout;
        self
    }

    /// Returns a copy of this context with unreaped worker count.
    ///
    /// # Arguments
    /// - `count`: Number of worker attempts not observed to exit after
    ///   cancellation.
    ///
    /// # Returns
    /// A context carrying the worker cleanup metric.
    #[inline(always)]
    pub(crate) fn with_unreaped_worker_count(mut self, count: u32) -> Self {
        self.unreaped_worker_count = count;
        self
    }
}
