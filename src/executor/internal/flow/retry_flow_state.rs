// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent state used by the retry flow controller.

#![cfg_attr(not(any(feature = "async", feature = "worker")), allow(dead_code))]

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
use crate::context::RetryContextParts;
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
    ///
    /// # Parameters
    /// - `started_at`: Initial sample defining the flow clock domain.
    /// - `policy`: Immutable policy borrowed for this flow.
    /// - `random_source`: Shared sampler for uniform delays and jitter.
    /// - `flow_timeout`: Optional hard whole-flow timeout.
    ///
    /// # Returns
    /// Fresh accounting and backoff state.
    #[inline]
    #[must_use = "use the prepared value or inspect the result"]
    pub(crate) fn new(
        started_at: MonotonicInstant,
        policy: &'a RetryPolicy,
        random_source: Option<Arc<dyn RetryRandomSource>>,
        flow_timeout: Option<Duration>,
    ) -> Self {
        Self {
            policy,
            budget: RetryBudgetState::new(
                started_at,
                *policy.admission_limits(),
            ),
            backoff: random_source.map_or_else(
                || policy.backoff().start(),
                |random_source| {
                    policy.backoff().start_with_random_source(random_source)
                },
            ),
            flow_timeout,
        }
    }

    /// Returns whether the hard flow timeout has expired.
    ///
    /// # Returns
    /// True when an enabled hard-flow limit has been reached.
    #[inline(always)]
    #[must_use]
    pub(crate) fn flow_timed_out(&self) -> bool {
        self.flow_timeout.is_some_and(|limit| {
            self.budget.snapshot().total_elapsed() >= limit
        })
    }

    /// Returns the first continuation limit that prevents another action.
    ///
    /// # Returns
    /// Some first exhausted continuation budget, or None if all admit another
    /// action.
    #[inline(always)]
    #[must_use]
    pub(crate) fn continuation_limit(&self) -> Option<RetryLimitKind> {
        self.budget.retry_limit(Duration::ZERO)
    }

    /// Checks whether a proposed retry delay fits the continuation limits.
    ///
    /// # Parameters
    /// - `delay`: Proposed next retry delay.
    ///
    /// # Returns
    /// Some first budget violated by continuing after this delay, or None if it
    /// fits.
    #[inline(always)]
    #[must_use]
    pub(crate) fn retry_limit(
        &self,
        delay: Duration,
    ) -> Option<RetryLimitKind> {
        self.budget.retry_limit(delay)
    }

    /// Returns time remaining before the hard flow timeout.
    ///
    /// # Returns
    /// Some saturating remaining time for a hard-flow limit, or None if
    /// unlimited.
    #[inline(always)]
    #[must_use]
    pub(crate) fn flow_remaining(&self) -> Option<Duration> {
        self.flow_timeout.map(|limit| {
            limit.saturating_sub(self.budget.snapshot().total_elapsed())
        })
    }

    /// Returns the absolute hard-flow deadline, when configured.
    ///
    /// # Errors
    /// Returns a clock overflow error when the configured duration cannot be
    /// represented in the flow's monotonic clock domain.
    ///
    /// # Returns
    /// Some absolute hard-flow deadline, or None when no limit is configured.
    #[inline]
    #[must_use = "use the prepared value or inspect the result"]
    pub(crate) fn flow_deadline(
        &self,
    ) -> Result<Option<MonotonicInstant>, TimeError> {
        self.flow_timeout
            .map(|timeout| self.budget.started_at().checked_add(timeout))
            .transpose()
    }

    /// Selects the source-aware timeout for the next attempt.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Per-attempt hard limit, or None.
    ///
    /// # Returns
    /// Some shortest enabled limit, or None without any hard timeout; ties
    /// belong to Attempt.
    #[inline(always)]
    #[must_use]
    pub(crate) fn effective_timeout(
        &self,
        attempt_timeout: Option<Duration>,
    ) -> Option<EffectiveTimeout> {
        EffectiveTimeout::select(attempt_timeout, self.flow_remaining())
    }

    /// Prepares an absolute deadline from the current control-boundary sample.
    #[inline]
    pub(crate) fn backoff_deadline(
        &self,
        now: MonotonicInstant,
        delay: Duration,
    ) -> Result<MonotonicInstant, TimeError> {
        let requested = now.checked_add(delay)?;
        let Some(flow_deadline) = self.flow_deadline()? else {
            return Ok(requested);
        };
        if requested.elapsed_since_origin()
            <= flow_deadline.elapsed_since_origin()
        {
            Ok(requested)
        } else {
            Ok(flow_deadline)
        }
    }

    /// Returns the next one-based attempt ordinal.
    ///
    /// # Returns
    /// The next nonzero ordinal, saturating at the largest representable
    /// attempt count.
    ///
    /// # Panics
    /// Panics only if the saturating ordinal calculation violates its nonzero
    /// invariant.
    #[inline]
    #[must_use]
    pub(crate) fn next_attempt(&self) -> NonZeroU32 {
        NonZeroU32::new(self.budget.attempts().saturating_add(1))
            .expect("an attempt ordinal is always non-zero")
    }

    /// Builds a context from the latest coherent state snapshot.
    ///
    /// # Parameters
    /// - `current_attempt`: Callback or operation ordinal, or None outside an
    ///   attempt.
    ///
    /// # Returns
    /// A coherent context without controller-specific timeout or scheduling
    /// overlays.
    #[inline]
    pub(crate) fn context(
        &self,
        current_attempt: Option<NonZeroU32>,
    ) -> RetryContext {
        let snapshot = self.budget.snapshot();
        RetryContext::from_parts(RetryContextParts {
            attempts: snapshot.attempts(),
            current_attempt,
            max_attempts: self.policy.admission_limits().max_attempts().get(),
            operation_time_budget: self
                .policy
                .admission_limits()
                .operation_time_budget(),
            total_time_budget: self
                .policy
                .admission_limits()
                .total_time_budget(),
            operation_elapsed: snapshot.operation_elapsed(),
            total_elapsed: snapshot.total_elapsed(),
            last_attempt_elapsed: snapshot.attempt_elapsed(),
            current_hard_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        })
    }

    /// Refreshes total elapsed time from `now`.
    ///
    /// # Errors
    /// Returns a clock error when `now` is from another domain or precedes the
    /// flow's initial sample.
    ///
    /// # Parameters
    /// - `now`: Latest same-domain monotonic sample.
    ///
    /// # Returns
    /// Unit after updating total elapsed time.
    #[inline(always)]
    pub(crate) fn refresh(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        self.budget.refresh(now)
    }

    /// Commits an attempt after the controller has validated its admission.
    ///
    /// # Parameters
    /// - `now`: Coherent admission sample after all gates pass.
    #[inline(always)]
    pub(crate) fn begin_attempt(&mut self, now: MonotonicInstant) {
        self.budget.begin_attempt(now);
    }

    /// Completes an admitted operation or returns an invalid-clock error.
    ///
    /// # Parameters
    /// - `now`: Completion sample for the active operation.
    ///
    /// # Returns
    /// Unit after committing operation and total elapsed time.
    ///
    /// # Errors
    /// Returns a clock-domain or regression error; no incoherent elapsed value
    /// is committed.
    #[inline(always)]
    pub(crate) fn finish_attempt(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        self.budget.finish_attempt(now)
    }

    /// Closes active accounting or refreshes idle accounting after
    /// infrastructure failure.
    ///
    /// # Parameters
    /// - `now`: Best available sample after runtime failure.
    ///
    /// # Returns
    /// Unit after closing active accounting or refreshing idle accounting.
    ///
    /// # Errors
    /// Returns the clock error when this sample cannot be used.
    #[inline]
    pub(crate) fn finish_for_infrastructure(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), TimeError> {
        if self.budget.has_active_attempt() {
            self.budget.finish_attempt(now)
        } else {
            self.budget.refresh(now)
        }
    }

    /// Selects and advances the next backoff step.
    ///
    /// # Parameters
    /// - `decision`: Retry rule decision carrying any hint.
    ///
    /// # Returns
    /// The next policy step; advances this flow’s sequence index.
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
}
