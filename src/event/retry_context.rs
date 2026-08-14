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

use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

use super::RetryContextParts;

/// Context emitted for retry lifecycle events.
///
/// `attempts` counts operations that actually started, while `current_attempt`
/// identifies the attempt associated with the current callback or operation.
/// `operation_elapsed` is cumulative user operation execution time only;
/// listener and retry-sleep time are excluded. `total_elapsed` is monotonic
/// elapsed time spent in the retry flow and includes operation execution,
/// retry sleep, retry-after sleep, and retry-control listener time.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryContext {
    /// Number of operations that actually started.
    attempts: u32,
    /// Current callback or operation attempt ordinal.
    current_attempt: Option<NonZeroU32>,
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
    /// Elapsed time spent in the last completed attempt.
    last_attempt_elapsed: Duration,
    /// Effective timeout configured for the current attempt.
    current_attempt_timeout: Option<Duration>,
    /// Delay selected before the next attempt, when known.
    next_delay: Option<Duration>,
    /// Optional retry-after hint extracted before failure policy runs.
    retry_after_hint: Option<Duration>,
}

impl RetryContext {
    /// Creates a public retry context snapshot with default timing metadata.
    ///
    /// # Arguments
    /// - `attempts`: Number of operations that actually started.
    /// - `max_attempts`: Configured maximum attempts.
    ///
    /// # Returns
    /// A retry context with no elapsed budgets, elapsed values, selected next
    /// delay, retry-after hint, or attempt timeout.
    pub fn new(attempts: u32, max_attempts: u32) -> Self {
        Self::from_parts(RetryContextParts {
            attempts,
            current_attempt: NonZeroU32::new(attempts),
            max_attempts,
            max_operation_elapsed: None,
            max_total_elapsed: None,
            operation_elapsed: Duration::ZERO,
            total_elapsed: Duration::ZERO,
            last_attempt_elapsed: Duration::ZERO,
            current_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        })
    }

    /// Creates a retry context snapshot from internal parts.
    ///
    /// # Arguments
    /// - `parts`: Internal context payload.
    ///
    /// # Returns
    /// A retry context containing exactly the supplied snapshot fields.
    pub(crate) fn from_parts(parts: RetryContextParts) -> Self {
        Self {
            attempts: parts.attempts,
            current_attempt: parts.current_attempt,
            max_attempts: parts.max_attempts,
            max_operation_elapsed: parts.max_operation_elapsed,
            max_total_elapsed: parts.max_total_elapsed,
            operation_elapsed: parts.operation_elapsed,
            total_elapsed: parts.total_elapsed,
            last_attempt_elapsed: parts.last_attempt_elapsed,
            current_attempt_timeout: parts.current_attempt_timeout,
            next_delay: parts.next_delay,
            retry_after_hint: parts.retry_after_hint,
        }
    }

    /// Returns the number of operations that actually started.
    ///
    /// # Returns
    /// The committed operation-attempt count. A callback before the first
    /// operation starts observes zero.
    #[inline(always)]
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Returns the attempt associated with the current operation or callback.
    ///
    /// # Returns
    /// `Some(NonZeroU32)` for an attempt-related snapshot, or `None` when no
    /// attempt is current.
    #[inline(always)]
    #[must_use]
    pub fn current_attempt(&self) -> Option<NonZeroU32> {
        self.current_attempt
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
        self.current_attempt.map_or(self.attempts, NonZeroU32::get)
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

    /// Returns elapsed time spent in the last completed attempt.
    ///
    /// # Returns
    /// Last completed attempt elapsed time. Before the first completed attempt,
    /// this is zero.
    #[inline(always)]
    #[must_use]
    pub fn last_attempt_elapsed(&self) -> Duration {
        self.last_attempt_elapsed
    }

    /// Returns elapsed time spent in the last completed attempt.
    ///
    /// This temporary accessor preserves the pre-migration executor API.
    ///
    /// # Returns
    /// The same value as [`Self::last_attempt_elapsed`].
    #[inline(always)]
    #[must_use]
    pub fn attempt_elapsed(&self) -> Duration {
        self.last_attempt_elapsed
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
    pub fn current_attempt_timeout(&self) -> Option<Duration> {
        self.current_attempt_timeout
    }

    /// Returns the effective timeout configured for the current attempt.
    ///
    /// This temporary accessor preserves the pre-migration executor API.
    ///
    /// # Returns
    /// The same value as [`Self::current_attempt_timeout`].
    #[inline(always)]
    #[must_use]
    pub fn attempt_timeout(&self) -> Option<Duration> {
        self.current_attempt_timeout
    }

    /// Returns the number of worker attempts not observed to exit after
    /// cancellation.
    ///
    /// # Returns
    /// Zero. Worker-stop details now belong to the terminal infrastructure
    /// failure value rather than retry context storage.
    #[inline(always)]
    #[must_use]
    pub fn unreaped_worker_count(&self) -> u32 {
        0
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
        self.current_attempt_timeout = timeout;
        self
    }

    /// Returns a copy of this context with unreaped worker count.
    ///
    /// # Arguments
    /// - `count`: Number of worker attempts not observed to exit after
    ///   cancellation.
    ///
    /// # Returns
    /// The unchanged context. Worker-stop details are represented by the
    /// terminal infrastructure failure value.
    #[inline(always)]
    #[allow(
        dead_code,
        reason = "legacy context shim remains until the T07 API removal"
    )]
    pub(crate) fn with_unreaped_worker_count(self, _count: u32) -> Self {
        self
    }
}
