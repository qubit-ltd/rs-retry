// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared continuation state for synchronous and timed retry executors.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;

use super::EffectiveTimeout;
use crate::BackoffRequest;
use crate::BackoffState;
use crate::BackoffStep;
use crate::RetryAttempt;
use crate::RetryBudget;
use crate::RetryBudgetError;
use crate::RetryBudgetExhausted;
use crate::RetryBudgetSnapshot;
use crate::RetryContext;
use crate::RetryErrorReason;
use crate::RetryPolicy;
use crate::RetryRandomSource;
use crate::event::RetryContextParts;
use crate::rule::RetryDecision;

/// Runtime state shared by every executor for one retry flow.
pub(crate) struct RetryFlowState<'a> {
    /// Immutable continuation and backoff policy.
    policy: &'a RetryPolicy,
    /// Single source of truth for attempt and elapsed continuation limits.
    budget: RetryBudget<'a>,
    /// Mutable backoff sequence.
    backoff: BackoffState,
    /// Optional hard deadline for the whole execution flow.
    flow_timeout: Option<Duration>,
}

impl<'a> RetryFlowState<'a> {
    /// Creates a flow state driven by the supplied clock and random source.
    pub(crate) fn new(
        clock: &'a dyn MonotonicClock,
        policy: &'a RetryPolicy,
        random_source: Arc<dyn RetryRandomSource>,
        flow_timeout: Option<Duration>,
    ) -> Result<Self, RetryBudgetError> {
        Ok(Self {
            policy,
            budget: RetryBudget::new(clock, *policy.limits())?,
            backoff: policy.backoff().start_with_random_source(random_source),
            flow_timeout,
        })
    }

    /// Returns the latest budget snapshot.
    pub(crate) fn snapshot(&self) -> RetryBudgetSnapshot {
        self.budget.snapshot()
    }

    /// Checks whether execution may continue immediately.
    pub(crate) fn continuation_reason(&self) -> Option<RetryErrorReason> {
        let snapshot = self.snapshot();
        if self
            .flow_timeout
            .is_some_and(|limit| snapshot.total_elapsed() >= limit)
        {
            return Some(RetryErrorReason::FlowTimedOut);
        }
        self.budget
            .check_retry_after(Duration::ZERO)
            .err()
            .map(Self::budget_reason)
    }

    /// Checks policy budgets for a prospective retry delay.
    pub(crate) fn retry_reason(
        &self,
        delay: Duration,
    ) -> Option<RetryErrorReason> {
        self.budget
            .check_retry_after(delay)
            .err()
            .map(Self::budget_reason)
    }

    /// Admits one attempt and returns its linear completion token.
    pub(crate) fn begin_attempt(
        &mut self,
    ) -> Result<RetryAttempt, RetryErrorReason> {
        if self
            .flow_remaining()
            .is_some_and(|remaining| remaining.is_zero())
        {
            return Err(RetryErrorReason::FlowTimedOut);
        }
        self.budget.begin_attempt().map_err(Self::budget_reason)
    }

    /// Finishes one admitted attempt and returns the updated snapshot.
    pub(crate) fn finish_attempt(
        &mut self,
        attempt: RetryAttempt,
    ) -> RetryBudgetSnapshot {
        self.budget.finish_attempt(attempt)
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

    /// Builds a context from a supplied budget snapshot and current ordinal.
    pub(crate) fn context(
        &self,
        snapshot: RetryBudgetSnapshot,
        current_attempt: u32,
    ) -> RetryContext {
        RetryContext::from_parts(RetryContextParts {
            attempts: snapshot.attempts(),
            current_attempt: NonZeroU32::new(current_attempt),
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

    /// Builds a context for the next attempt-start event.
    pub(crate) fn upcoming_context(&self) -> RetryContext {
        let snapshot = self.snapshot();
        let current_attempt = snapshot.attempts().saturating_add(1);
        self.context(snapshot, current_attempt)
    }

    /// Builds a context for the latest committed attempt count.
    pub(crate) fn current_context(&self) -> RetryContext {
        let snapshot = self.snapshot();
        self.context(snapshot, snapshot.attempts())
    }

    /// Returns time remaining before the hard flow deadline.
    pub(crate) fn flow_remaining(&self) -> Option<Duration> {
        let elapsed = self.snapshot().total_elapsed();
        self.flow_timeout.map(|limit| limit.saturating_sub(elapsed))
    }

    /// Selects the source-aware hard timeout for the next attempt.
    pub(crate) fn effective_timeout(
        &self,
        attempt_timeout: Option<Duration>,
    ) -> Option<EffectiveTimeout> {
        EffectiveTimeout::select(attempt_timeout, self.flow_remaining())
    }

    /// Returns the flow sleep cap when a delay reaches the hard deadline.
    pub(crate) fn flow_sleep_cap(&self, delay: Duration) -> Option<Duration> {
        self.flow_remaining()
            .filter(|remaining| delay >= *remaining)
    }

    /// Maps continuation budget exhaustion into the public error vocabulary.
    fn budget_reason(exhausted: RetryBudgetExhausted) -> RetryErrorReason {
        match exhausted {
            RetryBudgetExhausted::Attempts => {
                RetryErrorReason::AttemptsExhausted
            }
            RetryBudgetExhausted::OperationElapsed => {
                RetryErrorReason::OperationBudgetExhausted
            }
            RetryBudgetExhausted::TotalElapsed => {
                RetryErrorReason::TotalBudgetExhausted
            }
        }
    }
}
