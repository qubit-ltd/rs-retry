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
use crate::budget::RetryBudgetState;
use crate::event::RetryContextParts;
use crate::rule::RetryDecision;

/// Mutable timing, attempt, and backoff state for one retry flow.
pub(crate) struct RetryFlowState<'a> {
    /// Immutable continuation and backoff policy.
    policy: &'a RetryPolicy,
    /// Shared continuation and operation accounting.
    budget: RetryBudgetState,
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
            budget: RetryBudgetState::new(started_at, *policy.limits()),
            backoff: policy.backoff().start_with_random_source(random_source),
            flow_timeout,
        }
    }

    /// Refreshes total elapsed time from `now`.
    ///
    /// # Errors
    /// Returns a clock error when `now` is from another domain or precedes the
    /// flow's initial sample.
    pub(crate) fn refresh(&mut self, now: MonotonicInstant) -> Result<(), TimeError> {
        self.budget.refresh(now)
    }

    /// Returns whether the hard flow timeout has expired.
    pub(crate) fn flow_timed_out(&self) -> bool {
        self.flow_timeout
            .is_some_and(|limit| self.budget.snapshot().total_elapsed() >= limit)
    }

    /// Returns the first continuation limit that prevents another action.
    pub(crate) fn continuation_limit(&self) -> Option<RetryLimitKind> {
        self.budget.retry_limit(Duration::ZERO)
    }

    /// Checks whether a proposed retry delay fits the continuation limits.
    pub(crate) fn retry_limit(&self, delay: Duration) -> Option<RetryLimitKind> {
        self.budget.retry_limit(delay)
    }

    /// Commits an attempt after the controller has validated its admission.
    pub(crate) fn begin_attempt(&mut self, now: MonotonicInstant) {
        self.budget.begin_attempt(now);
    }

    /// Completes an admitted operation or returns an invalid-clock error.
    pub(crate) fn finish_attempt(&mut self, now: MonotonicInstant) -> Result<(), TimeError> {
        self.budget.finish_attempt(now)
    }

    /// Closes active accounting or refreshes idle accounting after
    /// infrastructure failure.
    pub(crate) fn finish_for_infrastructure(&mut self, now: MonotonicInstant) -> Result<(), TimeError> {
        if self.budget.has_active_attempt() {
            self.budget.finish_attempt(now)
        } else {
            self.budget.refresh(now)
        }
    }

    /// Selects and advances the next backoff step.
    pub(crate) fn next_backoff(&mut self, decision: RetryDecision) -> BackoffStep {
        let request = match decision {
            RetryDecision::RetryWithHint(delay) => BackoffRequest::hint(delay),
            RetryDecision::RetryWithJitteredHint(delay) => BackoffRequest::jittered_hint(delay),
            RetryDecision::Retry | RetryDecision::UseDefault | RetryDecision::Abort => BackoffRequest::policy(),
        };
        self.backoff.next(request)
    }

    /// Returns time remaining before the hard flow timeout.
    pub(crate) fn flow_remaining(&self) -> Option<Duration> {
        self.flow_timeout
            .map(|limit| limit.saturating_sub(self.budget.snapshot().total_elapsed()))
    }

    /// Returns the absolute hard-flow deadline, when configured.
    ///
    /// # Errors
    /// Returns a clock overflow error when the configured duration cannot be
    /// represented in the flow's monotonic clock domain.
    pub(crate) fn flow_deadline(&self) -> Result<Option<MonotonicInstant>, TimeError> {
        self.flow_timeout
            .map(|timeout| self.budget.started_at().checked_add(timeout))
            .transpose()
    }

    /// Selects the source-aware timeout for the next attempt.
    pub(crate) fn effective_timeout(&self, attempt_timeout: Option<Duration>) -> Option<EffectiveTimeout> {
        EffectiveTimeout::select(attempt_timeout, self.flow_remaining())
    }

    /// Caps a retry sleep at the remaining hard flow timeout.
    pub(crate) fn sleep_duration(&self, delay: Duration) -> Duration {
        self.flow_remaining().map_or(delay, |remaining| delay.min(remaining))
    }

    /// Returns the next one-based attempt ordinal.
    pub(crate) fn next_attempt(&self) -> NonZeroU32 {
        NonZeroU32::new(self.budget.attempts().saturating_add(1)).expect("an attempt ordinal is always non-zero")
    }

    /// Builds a context from the latest coherent state snapshot.
    pub(crate) fn context(&self, current_attempt: Option<NonZeroU32>) -> RetryContext {
        let snapshot = self.budget.snapshot();
        RetryContext::from_parts(RetryContextParts {
            attempts: snapshot.attempts(),
            current_attempt,
            max_attempts: self.policy.limits().max_attempts().get(),
            max_operation_elapsed: self.policy.limits().max_operation_elapsed(),
            max_total_elapsed: self.policy.limits().max_total_elapsed(),
            operation_elapsed: snapshot.operation_elapsed(),
            total_elapsed: snapshot.total_elapsed(),
            last_attempt_elapsed: snapshot.attempt_elapsed(),
            current_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        })
    }
}
