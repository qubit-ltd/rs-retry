// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent state used by the retry flow controller.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;

use super::EffectiveTimeout;
use crate::BackoffRequest;
use crate::BackoffState;
use crate::BackoffStep;
use crate::RetryContext;
use crate::RetryLimitKind;
use crate::RetryPolicy;
use crate::RetryRandomSource;
use crate::event::RetryContextParts;
use crate::rule::RetryDecision;

/// Mutable timing, attempt, and backoff state for one retry flow.
pub(crate) struct RetryFlowState<'a> {
    /// Immutable continuation and backoff policy.
    policy: &'a RetryPolicy,
    /// First monotonic sample for the flow.
    started_at: MonotonicInstant,
    /// Latest total elapsed snapshot.
    total_elapsed: Duration,
    /// Number of admitted operations.
    attempts: u32,
    /// Cumulative elapsed operation time.
    operation_elapsed: Duration,
    /// Duration of the latest completed operation.
    last_attempt_elapsed: Duration,
    /// Start sample for the active attempt, if any.
    attempt_started_at: Option<MonotonicInstant>,
    /// Mutable backoff sequence.
    backoff: BackoffState,
    /// Optional hard timeout for the complete flow.
    flow_timeout: Option<Duration>,
}

impl<'a> RetryFlowState<'a> {
    /// Creates state from one coherent initial monotonic sample.
    pub(crate) fn new(
        started_at: MonotonicInstant,
        policy: &'a RetryPolicy,
        random_source: Arc<dyn RetryRandomSource>,
        flow_timeout: Option<Duration>,
    ) -> Self {
        Self {
            policy,
            started_at,
            total_elapsed: Duration::ZERO,
            attempts: 0,
            operation_elapsed: Duration::ZERO,
            last_attempt_elapsed: Duration::ZERO,
            attempt_started_at: None,
            backoff: policy.backoff().start_with_random_source(random_source),
            flow_timeout,
        }
    }

    /// Refreshes total elapsed time from `now`.
    ///
    /// # Errors
    /// Returns a clock error when `now` is from another domain or precedes the
    /// flow's initial sample.
    pub(crate) fn refresh(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        self.total_elapsed = now.duration_since(self.started_at)?;
        Ok(())
    }

    /// Returns whether the hard flow timeout has expired.
    pub(crate) fn flow_timed_out(&self) -> bool {
        self.flow_timeout
            .is_some_and(|limit| self.total_elapsed >= limit)
    }

    /// Returns the first continuation limit that prevents another action.
    pub(crate) fn continuation_limit(&self) -> Option<RetryLimitKind> {
        let limits = self.policy.limits();
        if self.attempts >= limits.max_attempts().get() {
            return Some(RetryLimitKind::Attempts);
        }
        if limits
            .max_operation_elapsed()
            .is_some_and(|limit| self.operation_elapsed >= limit)
        {
            return Some(RetryLimitKind::OperationElapsed);
        }
        if limits
            .max_total_elapsed()
            .is_some_and(|limit| self.total_elapsed >= limit)
        {
            return Some(RetryLimitKind::TotalElapsed);
        }
        None
    }

    /// Checks whether a prospective retry delay remains inside all limits.
    pub(crate) fn retry_limit(
        &self,
        delay: Duration,
    ) -> Option<RetryLimitKind> {
        if let Some(limit) = self.continuation_limit() {
            return Some(limit);
        }
        self.policy
            .limits()
            .max_total_elapsed()
            .filter(|limit| self.total_elapsed.saturating_add(delay) >= *limit)
            .map(|_| RetryLimitKind::TotalElapsed)
    }

    /// Commits an already-checked attempt and starts its elapsed measurement.
    pub(crate) fn begin_attempt(&mut self, now: MonotonicInstant) {
        debug_assert!(self.attempt_started_at.is_none());
        self.attempts = self.attempts.saturating_add(1);
        self.attempt_started_at = Some(now);
    }

    /// Finishes the active attempt and refreshes all elapsed snapshots.
    ///
    /// # Errors
    /// Returns a clock error when the supplied sample is invalid for the
    /// flow's clock domain or precedes the attempt start.
    pub(crate) fn finish_attempt(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        let started_at = self
            .attempt_started_at
            .as_ref()
            .expect("an admitted attempt must be active before completion");
        let total_elapsed = now.duration_since(self.started_at)?;
        let elapsed = now.duration_since(*started_at)?;
        self.total_elapsed = total_elapsed;
        self.last_attempt_elapsed = elapsed;
        self.operation_elapsed = self.operation_elapsed.saturating_add(elapsed);
        self.attempt_started_at = None;
        Ok(())
    }

    /// Refreshes elapsed time for a terminal infrastructure event.
    ///
    /// An active attempt is closed so its elapsed duration appears in the
    /// terminal context even when runtime mechanics failed.
    pub(crate) fn finish_for_infrastructure(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        if self.attempt_started_at.is_some() {
            self.finish_attempt(now)
        } else {
            self.refresh(now)
        }
    }

    /// Selects and advances the next backoff step.
    pub(crate) fn next_backoff(
        &mut self,
        decision: RetryDecision,
    ) -> BackoffStep {
        let request = match decision {
            RetryDecision::RetryWithHint(delay) => BackoffRequest::hint(delay),
            RetryDecision::RetryWithJitteredHint(delay) => {
                BackoffRequest::jittered_hint(delay)
            }
            RetryDecision::Retry
            | RetryDecision::UseDefault
            | RetryDecision::Abort => BackoffRequest::policy(),
        };
        self.backoff.next(request)
    }

    /// Returns time remaining before the hard flow timeout.
    pub(crate) fn flow_remaining(&self) -> Option<Duration> {
        self.flow_timeout
            .map(|limit| limit.saturating_sub(self.total_elapsed))
    }

    /// Returns the absolute hard-flow deadline, when configured.
    ///
    /// # Errors
    /// Returns a clock overflow error when the configured duration cannot be
    /// represented in the flow's monotonic clock domain.
    #[cfg(feature = "tokio")]
    pub(crate) fn flow_deadline(
        &self,
    ) -> Result<Option<MonotonicInstant>, TimeError> {
        self.flow_timeout
            .map(|timeout| self.started_at.checked_add(timeout))
            .transpose()
    }

    /// Selects the source-aware timeout for the next attempt.
    pub(crate) fn effective_timeout(
        &self,
        attempt_timeout: Option<Duration>,
    ) -> Option<EffectiveTimeout> {
        EffectiveTimeout::select(attempt_timeout, self.flow_remaining())
    }

    /// Caps a retry sleep at the remaining hard flow timeout.
    pub(crate) fn sleep_duration(&self, delay: Duration) -> Duration {
        self.flow_remaining()
            .map_or(delay, |remaining| delay.min(remaining))
    }

    /// Returns the next one-based attempt ordinal.
    pub(crate) fn next_attempt(&self) -> NonZeroU32 {
        NonZeroU32::new(self.attempts.saturating_add(1))
            .expect("an attempt ordinal is always non-zero")
    }

    /// Builds a context from the latest coherent state snapshot.
    pub(crate) fn context(
        &self,
        current_attempt: Option<NonZeroU32>,
    ) -> RetryContext {
        RetryContext::from_parts(RetryContextParts {
            attempts: self.attempts,
            current_attempt,
            max_attempts: self.policy.limits().max_attempts().get(),
            max_operation_elapsed: self.policy.limits().max_operation_elapsed(),
            max_total_elapsed: self.policy.limits().max_total_elapsed(),
            operation_elapsed: self.operation_elapsed,
            total_elapsed: self.total_elapsed,
            last_attempt_elapsed: self.last_attempt_elapsed,
            current_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        })
    }
}
