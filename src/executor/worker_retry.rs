// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Worker-thread execution facade for blocking operations.

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::BlockingSleeper;
use qubit_clock::StdTimer;
use qubit_clock::TimeError;
use qubit_clock::Timer;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_value_operation::BlockingValueOperation;
use super::retry::Retry;
use super::worker_attempt_executor::WorkerAttemptExecutor;
use crate::AttemptFailure;
use crate::AttemptTimeoutKind;
use crate::BackoffRequest;
use crate::RetryBudget;
use crate::RetryBudgetExhausted;
use crate::RetryBudgetSnapshot;
use crate::RetryContext;
use crate::RetryError;
use crate::RetryErrorReason;
use crate::RetryPolicy;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::event::RetryContextParts;
use crate::observer::RetryOutcomeKind;
use crate::random::ThreadRetryRandomSource;
use crate::rule::RetryDecision;

/// Worker retry execution with cooperative cancellation.
pub struct WorkerRetry<'a, E> {
    retry: &'a Retry<E>,
    attempt_timeout: Option<Duration>,
    flow_timeout: Option<Duration>,
    cancellation_grace: Duration,
    sleeper: BlockingSleeper,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: Send + 'static> WorkerRetry<'a, E> {
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            attempt_timeout: None,
            flow_timeout: None,
            cancellation_grace: Duration::from_millis(100),
            sleeper: BlockingSleeper::new(Arc::new(StdTimer::new())),
            random_source: Arc::new(ThreadRetryRandomSource),
        }
    }

    /// Sets the maximum duration of one worker attempt.
    pub fn attempt_timeout(mut self, timeout: Duration) -> Self {
        self.attempt_timeout = Some(timeout);
        self
    }

    /// Sets the wall-clock timeout for the complete flow.
    pub fn flow_timeout(mut self, timeout: Duration) -> Self {
        self.flow_timeout = Some(timeout);
        self
    }

    /// Sets the grace period used after requesting cooperative cancellation.
    pub fn cancellation_grace(mut self, grace: Duration) -> Self {
        self.cancellation_grace = grace;
        self
    }

    /// Injects the blocking timer and clock.
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.sleeper = BlockingSleeper::new(timer);
        self
    }

    /// Injects the random source used by backoff jitter.
    pub fn random_source(
        mut self,
        random_source: Arc<dyn RetryRandomSource>,
    ) -> Self {
        self.random_source = random_source;
        self
    }

    /// Runs one cancellable worker operation per attempt.
    #[allow(clippy::result_large_err)]
    pub fn run<T, F>(
        &self,
        operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        T: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let operation = Arc::new(BlockingValueOperation::new(operation));
        let worker_operation: Arc<dyn BlockingAttempt<E>> = operation.clone();
        let timer = self.sleeper.timer();
        let clock = timer.clock();
        let mut budget = RetryBudget::new(clock, *self.retry.policy().limits())
            .expect("validated retry limits must fit the monotonic clock");
        let mut last_failure = None;
        let mut backoff = self
            .retry
            .policy()
            .backoff()
            .start_with_random_source(Arc::clone(&self.random_source));

        loop {
            let snapshot = budget.snapshot();
            if let Some(reason) = self
                .flow_timeout_reason(snapshot.total_elapsed())
                .or_else(|| {
                    budget
                        .check_retry_after(Duration::ZERO)
                        .err()
                        .map(retry_budget_reason)
                })
            {
                let context = context(
                    self.retry.policy(),
                    snapshot,
                    snapshot.attempts(),
                    None,
                );
                return Err(self.finish(reason, last_failure, context));
            }
            let before = context(
                self.retry.policy(),
                snapshot,
                snapshot.attempts().saturating_add(1),
                None,
            );
            self.retry.observers().attempt_started(&before);
            let snapshot = budget.snapshot();
            if let Some(reason) = self
                .flow_timeout_reason(snapshot.total_elapsed())
                .or_else(|| {
                    budget
                        .check_retry_after(Duration::ZERO)
                        .err()
                        .map(retry_budget_reason)
                })
            {
                let context = context(
                    self.retry.policy(),
                    snapshot,
                    snapshot.attempts(),
                    None,
                );
                return Err(self.finish(reason, last_failure, context));
            }

            let attempt = match budget.begin_attempt() {
                Ok(attempt) => attempt,
                Err(exhausted) => {
                    let snapshot = budget.snapshot();
                    return Err(self.finish(
                        retry_budget_reason(exhausted),
                        last_failure,
                        context(
                            self.retry.policy(),
                            snapshot,
                            snapshot.attempts(),
                            None,
                        ),
                    ));
                }
            };
            let effective_timeout =
                self.effective_timeout(snapshot.total_elapsed());
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&worker_operation),
                effective_timeout,
                self.cancellation_grace,
            );
            let snapshot = budget.finish_attempt(attempt);
            let mut attempt_context = context(
                self.retry.policy(),
                snapshot,
                snapshot.attempts(),
                None,
            )
            .with_attempt_timeout(effective_timeout)
            .with_unreaped_worker_count(outcome.unreaped_worker_count);
            let result = match outcome.result {
                Ok(()) => Ok(()),
                Err(AttemptFailure::Timeout { .. }) => {
                    let kind = if self
                        .flow_timeout
                        .is_some_and(|limit| snapshot.total_elapsed() >= limit)
                        && self.attempt_timeout.is_none_or(|limit| {
                            snapshot.total_elapsed() >= limit
                        }) {
                        AttemptTimeoutKind::Flow
                    } else {
                        AttemptTimeoutKind::Attempt
                    };
                    Err(AttemptFailure::Timeout { kind })
                }
                Err(failure) => Err(failure),
            };
            match result {
                Ok(()) => {
                    self.retry.observers().finished(
                        RetryOutcomeKind::Succeeded,
                        &attempt_context,
                    );
                    return Ok(RetrySuccess::new(
                        operation.take_value(),
                        attempt_context,
                    ));
                }
                Err(failure) => {
                    self.retry
                        .observers()
                        .attempt_failed(&failure, &attempt_context);
                    if attempt_context.unreaped_worker_count() > 0 {
                        return Err(self.finish(
                            RetryErrorReason::WorkerStillRunning,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    let mut diagnostics = Vec::new();
                    let decision = self.retry.rules().decide(
                        &failure,
                        &attempt_context,
                        &mut diagnostics,
                    );
                    for diagnostic in &diagnostics {
                        self.retry.observers().diagnostic(
                            diagnostic,
                            &attempt_context,
                            None,
                        );
                    }
                    let decision = default_decision(decision, &failure);
                    if matches!(
                        failure,
                        AttemptFailure::Timeout {
                            kind: AttemptTimeoutKind::Flow
                        }
                    ) {
                        return Err(self.finish(
                            RetryErrorReason::FlowTimedOut,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if matches!(decision, RetryDecision::Abort) {
                        return Err(self.finish(
                            terminal_reason(&failure),
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if let Some(reason) = self
                        .flow_timeout_reason(snapshot.total_elapsed())
                        .or_else(|| {
                            budget
                                .check_retry_after(Duration::ZERO)
                                .err()
                                .map(retry_budget_reason)
                        })
                    {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    let request = match decision {
                        RetryDecision::RetryAfter(delay) => {
                            BackoffRequest::explicit(delay)
                        }
                        RetryDecision::Retry | RetryDecision::UseDefault => {
                            BackoffRequest::policy()
                        }
                        RetryDecision::Abort => BackoffRequest::policy(),
                    };
                    let step = backoff.next(request);
                    attempt_context =
                        attempt_context.with_next_delay(step.effective_delay());
                    self.retry
                        .observers()
                        .retry_scheduled(&step, &attempt_context);
                    if let Some(reason) = self
                        .flow_timeout_reason(budget.snapshot().total_elapsed())
                        .or_else(|| {
                            budget
                                .check_retry_after(step.effective_delay())
                                .err()
                                .map(retry_budget_reason)
                        })
                    {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if let Err(error) =
                        self.sleeper.sleep_for(step.effective_delay())
                    {
                        return Err(self.finish_with_timer_error(
                            Some(failure),
                            attempt_context,
                            error,
                        ));
                    }
                    last_failure = Some(failure);
                }
            }
        }
    }

    fn effective_timeout(&self, total_elapsed: Duration) -> Option<Duration> {
        let flow_remaining = self
            .flow_timeout
            .map(|limit| limit.saturating_sub(total_elapsed));
        match (self.attempt_timeout, flow_remaining) {
            (Some(attempt), Some(flow)) => Some(attempt.min(flow)),
            (Some(attempt), None) => Some(attempt),
            (None, Some(flow)) => Some(flow),
            (None, None) => None,
        }
    }

    fn flow_timeout_reason(
        &self,
        total_elapsed: Duration,
    ) -> Option<RetryErrorReason> {
        self.flow_timeout
            .is_some_and(|limit| total_elapsed >= limit)
            .then_some(RetryErrorReason::FlowTimedOut)
    }

    fn finish(
        &self,
        reason: RetryErrorReason,
        failure: Option<AttemptFailure<E>>,
        context: RetryContext,
    ) -> RetryError<E> {
        let error = RetryError::new(reason, failure, context);
        self.retry
            .observers()
            .finished(RetryOutcomeKind::Failed, error.context());
        error
    }

    fn finish_with_timer_error(
        &self,
        failure: Option<AttemptFailure<E>>,
        context: RetryContext,
        error: TimeError,
    ) -> RetryError<E> {
        let error = RetryError::new_with_execution_error(
            RetryErrorReason::TimerFailed,
            failure,
            crate::RetryExecutionError::timer(&error.to_string()),
            context,
        );
        self.retry
            .observers()
            .finished(RetryOutcomeKind::Failed, error.context());
        error
    }
}

fn default_decision<E>(
    decision: RetryDecision,
    failure: &AttemptFailure<E>,
) -> RetryDecision {
    if !matches!(decision, RetryDecision::UseDefault) {
        return decision;
    }
    match failure {
        AttemptFailure::Error(_) => RetryDecision::Retry,
        AttemptFailure::Timeout { .. }
        | AttemptFailure::Panic
        | AttemptFailure::Infrastructure(_) => RetryDecision::Abort,
    }
}

fn terminal_reason<E>(failure: &AttemptFailure<E>) -> RetryErrorReason {
    match failure {
        AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Attempt,
        } => RetryErrorReason::AttemptTimedOut,
        AttemptFailure::Error(_)
        | AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Flow,
        }
        | AttemptFailure::Panic
        | AttemptFailure::Infrastructure(_) => RetryErrorReason::Aborted,
    }
}

/// Maps public continuation exhaustion to the retry error vocabulary.
fn retry_budget_reason(exhausted: RetryBudgetExhausted) -> RetryErrorReason {
    match exhausted {
        RetryBudgetExhausted::Attempts => RetryErrorReason::AttemptsExhausted,
        RetryBudgetExhausted::OperationElapsed => {
            RetryErrorReason::OperationBudgetExhausted
        }
        RetryBudgetExhausted::TotalElapsed => {
            RetryErrorReason::TotalBudgetExhausted
        }
    }
}

fn context(
    policy: &RetryPolicy,
    snapshot: RetryBudgetSnapshot,
    attempt: u32,
    next_delay: Option<Duration>,
) -> RetryContext {
    let mut context = RetryContext::from_parts(RetryContextParts {
        attempt,
        max_attempts: policy.limits().max_attempts().get(),
        max_operation_elapsed: policy.limits().max_operation_elapsed(),
        max_total_elapsed: policy.limits().max_total_elapsed(),
        operation_elapsed: snapshot.operation_elapsed(),
        total_elapsed: snapshot.total_elapsed(),
        attempt_elapsed: snapshot.attempt_elapsed(),
        attempt_timeout: None,
    });
    if let Some(delay) = next_delay {
        context = context.with_next_delay(delay);
    }
    context
}
