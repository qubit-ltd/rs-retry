// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Same-thread execution facade for the pure retry policy API.

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::BlockingSleeper;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::StdTimer;
use qubit_clock::Timer;

use super::retry::Retry;
use crate::AttemptFailure;
use crate::BackoffRequest;
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

/// Same-thread retry execution. It intentionally exposes no timeout method.
pub struct SyncRetry<'a, E> {
    retry: &'a Retry<E>,
    sleeper: BlockingSleeper,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> SyncRetry<'a, E> {
    /// Creates a synchronous facade from one retry policy.
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            sleeper: BlockingSleeper::new(Arc::new(StdTimer::new())),
            random_source: Arc::new(ThreadRetryRandomSource),
        }
    }

    /// Replaces the blocking timer used by this execution.
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.sleeper = BlockingSleeper::new(timer);
        self
    }

    /// Replaces the random source used by this execution.
    pub fn random_source(mut self, random: Arc<dyn RetryRandomSource>) -> Self {
        self.random_source = random;
        self
    }

    /// Runs a same-thread operation until success or a terminal retry error.
    #[allow(clippy::result_large_err)]
    pub fn run<T, F>(
        &self,
        mut operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let policy = self.retry.policy();
        let limits = policy.limits();
        let clock = self.sleeper.timer().clock();
        let started_at = clock.now();
        let mut attempts = 0_u32;
        let mut operation_elapsed = Duration::ZERO;
        let mut backoff = policy
            .backoff()
            .start_with_random_source(Arc::clone(&self.random_source));

        loop {
            let total_elapsed = elapsed_since(clock, started_at);
            if attempts >= limits.max_attempts().get()
                || limits
                    .max_operation_elapsed()
                    .is_some_and(|limit| operation_elapsed >= limit)
                || limits
                    .max_total_elapsed()
                    .is_some_and(|limit| total_elapsed >= limit)
            {
                let reason =
                    budget_reason(limits, operation_elapsed, total_elapsed);
                let context = context(
                    policy,
                    attempts,
                    operation_elapsed,
                    total_elapsed,
                    Duration::ZERO,
                    None,
                );
                let error = RetryError::new(reason, None, context);
                self.retry
                    .observers()
                    .finished(RetryOutcomeKind::Failed, &error_context(&error));
                return Err(error);
            }

            let upcoming = context(
                policy,
                attempts.saturating_add(1),
                operation_elapsed,
                total_elapsed,
                Duration::ZERO,
                None,
            );
            self.retry.observers().attempt_started(&upcoming);
            let total_elapsed = elapsed_since(clock, started_at);
            if attempts >= limits.max_attempts().get()
                || limits
                    .max_operation_elapsed()
                    .is_some_and(|limit| operation_elapsed >= limit)
                || limits
                    .max_total_elapsed()
                    .is_some_and(|limit| total_elapsed >= limit)
            {
                let context = context(
                    policy,
                    attempts,
                    operation_elapsed,
                    total_elapsed,
                    Duration::ZERO,
                    None,
                );
                let reason =
                    budget_reason(limits, operation_elapsed, total_elapsed);
                let error = RetryError::new(reason, None, context);
                self.retry
                    .observers()
                    .finished(RetryOutcomeKind::Failed, error.context());
                return Err(error);
            }

            attempts = attempts.saturating_add(1);
            let attempt_started = clock.now();
            let result = operation();
            let attempt_elapsed = elapsed_since(clock, attempt_started);
            operation_elapsed =
                operation_elapsed.saturating_add(attempt_elapsed);
            let total_elapsed = elapsed_since(clock, started_at);
            let attempt_context = context(
                policy,
                attempts,
                operation_elapsed,
                total_elapsed,
                attempt_elapsed,
                None,
            );

            match result {
                Ok(value) => {
                    self.retry.observers().finished(
                        RetryOutcomeKind::Succeeded,
                        &attempt_context,
                    );
                    return Ok(RetrySuccess::new(value, attempt_context));
                }
                Err(error) => {
                    let failure = AttemptFailure::Error(error);
                    self.retry
                        .observers()
                        .attempt_failed(&failure, &attempt_context);
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
                    if matches!(decision, RetryDecision::Abort) {
                        let error = RetryError::new(
                            RetryErrorReason::Aborted,
                            Some(failure),
                            attempt_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if attempts >= limits.max_attempts().get() {
                        let error = RetryError::new(
                            RetryErrorReason::AttemptsExhausted,
                            Some(failure),
                            attempt_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if operation_elapsed
                        >= limits
                            .max_operation_elapsed()
                            .unwrap_or(Duration::MAX)
                    {
                        let error = RetryError::new(
                            RetryErrorReason::OperationBudgetExhausted,
                            Some(failure),
                            attempt_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if total_elapsed
                        >= limits.max_total_elapsed().unwrap_or(Duration::MAX)
                    {
                        let error = RetryError::new(
                            RetryErrorReason::TotalBudgetExhausted,
                            Some(failure),
                            attempt_context,
                        );
                        return Err(error);
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
                    let scheduled_context = context(
                        policy,
                        attempts,
                        operation_elapsed,
                        elapsed_since(clock, started_at),
                        attempt_elapsed,
                        Some(step.effective_delay()),
                    );
                    self.retry
                        .observers()
                        .retry_scheduled(&step, &scheduled_context);
                    let total_elapsed = elapsed_since(clock, started_at);
                    if limits.max_total_elapsed().is_some_and(|limit| {
                        total_elapsed.saturating_add(step.effective_delay())
                            >= limit
                    }) {
                        let error = RetryError::new(
                            RetryErrorReason::TotalBudgetExhausted,
                            Some(failure),
                            scheduled_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if let Err(timer_error) =
                        self.sleeper.sleep_for(step.effective_delay())
                    {
                        let error = RetryError::new_with_execution_error(
                            RetryErrorReason::TimerFailed,
                            Some(failure),
                            crate::RetryExecutionError::timer(
                                &timer_error.to_string(),
                            ),
                            scheduled_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                }
            }
        }
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

fn elapsed_since(
    clock: &dyn MonotonicClock,
    started: MonotonicInstant,
) -> Duration {
    clock
        .now()
        .duration_since(started)
        .expect("retry clock must be monotonic")
}

fn budget_reason(
    limits: &crate::RetryLimits,
    operation_elapsed: Duration,
    total_elapsed: Duration,
) -> RetryErrorReason {
    if limits.max_attempts().get() == 0 {
        RetryErrorReason::AttemptsExhausted
    } else if limits
        .max_operation_elapsed()
        .is_some_and(|limit| operation_elapsed >= limit)
    {
        RetryErrorReason::OperationBudgetExhausted
    } else if limits
        .max_total_elapsed()
        .is_some_and(|limit| total_elapsed >= limit)
    {
        RetryErrorReason::TotalBudgetExhausted
    } else {
        RetryErrorReason::AttemptsExhausted
    }
}

fn context(
    policy: &RetryPolicy,
    attempt: u32,
    operation_elapsed: Duration,
    total_elapsed: Duration,
    attempt_elapsed: Duration,
    next_delay: Option<Duration>,
) -> RetryContext {
    let mut context = RetryContext::from_parts(RetryContextParts {
        attempt,
        max_attempts: policy.limits().max_attempts().get(),
        max_operation_elapsed: policy.limits().max_operation_elapsed(),
        max_total_elapsed: policy.limits().max_total_elapsed(),
        operation_elapsed,
        total_elapsed,
        attempt_elapsed,
        attempt_timeout: None,
    });
    if let Some(delay) = next_delay {
        context = context.with_next_delay(delay);
    }
    context
}

fn error_context<E>(error: &RetryError<E>) -> RetryContext {
    *error.context()
}
