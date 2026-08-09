// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Tokio execution facade for the policy-based retry API.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TokioTimer;

use super::retry::Retry;
use crate::AttemptFailure;
use crate::AttemptTimeoutKind;
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

/// Tokio retry execution with explicit attempt and flow timeout controls.
pub struct AsyncRetry<'a, E> {
    retry: &'a Retry<E>,
    attempt_timeout: Option<Duration>,
    flow_timeout: Option<Duration>,
    timer: Option<Arc<dyn Timer>>,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> AsyncRetry<'a, E> {
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            attempt_timeout: None,
            flow_timeout: None,
            timer: None,
            random_source: Arc::new(ThreadRetryRandomSource),
        }
    }

    /// Sets the maximum duration of one admitted attempt.
    pub fn attempt_timeout(mut self, timeout: Duration) -> Self {
        self.attempt_timeout = Some(timeout);
        self
    }

    /// Sets the wall-clock timeout for the entire flow.
    pub fn flow_timeout(mut self, timeout: Duration) -> Self {
        self.flow_timeout = Some(timeout);
        self
    }

    /// Injects a timer and clock, primarily for deterministic tests.
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.timer = Some(timer);
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

    /// Executes one future per attempt.
    pub async fn run<T, F, Fut>(
        &self,
        mut operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let timer = self
            .timer
            .clone()
            .unwrap_or_else(|| Arc::new(TokioTimer::current()));
        let clock = timer.clock();
        let started_at = clock.now();
        let mut attempts = 0_u32;
        let mut operation_elapsed = Duration::ZERO;
        let mut last_failure = None;
        let mut backoff = self
            .retry
            .policy()
            .backoff()
            .start_with_random_source(Arc::clone(&self.random_source));

        loop {
            let total_elapsed = elapsed_since(clock, started_at);
            if let Some(reason) = self
                .budget_reason(attempts, operation_elapsed, total_elapsed)
                .or_else(|| self.flow_timeout_reason(total_elapsed))
            {
                return Err(self.finish(
                    reason,
                    last_failure,
                    context(
                        self.retry.policy(),
                        attempts,
                        operation_elapsed,
                        total_elapsed,
                        Duration::ZERO,
                        None,
                    ),
                ));
            }

            let before = context(
                self.retry.policy(),
                attempts.saturating_add(1),
                operation_elapsed,
                total_elapsed,
                Duration::ZERO,
                None,
            );
            self.retry.observers().attempt_started(&before);
            let total_elapsed = elapsed_since(clock, started_at);
            if let Some(reason) = self
                .budget_reason(attempts, operation_elapsed, total_elapsed)
                .or_else(|| self.flow_timeout_reason(total_elapsed))
            {
                return Err(self.finish(
                    reason,
                    last_failure,
                    context(
                        self.retry.policy(),
                        attempts,
                        operation_elapsed,
                        total_elapsed,
                        Duration::ZERO,
                        None,
                    ),
                ));
            }

            attempts = attempts.saturating_add(1);
            let attempt_started = clock.now();
            let timeout = self.effective_timeout(total_elapsed);
            let outcome = execute_attempt(
                &timer,
                timeout,
                self.attempt_timeout,
                operation(),
            )
            .await;
            let attempt_elapsed = elapsed_since(clock, attempt_started);
            operation_elapsed =
                operation_elapsed.saturating_add(attempt_elapsed);
            let total_elapsed = elapsed_since(clock, started_at);
            let attempt_context = context(
                self.retry.policy(),
                attempts,
                operation_elapsed,
                total_elapsed,
                attempt_elapsed,
                None,
            )
            .with_attempt_timeout(timeout);

            match outcome {
                Ok(value) => {
                    self.retry.observers().finished(
                        RetryOutcomeKind::Succeeded,
                        &attempt_context,
                    );
                    return Ok(RetrySuccess::new(value, attempt_context));
                }
                Err(failure) => {
                    self.retry
                        .observers()
                        .attempt_failed(&failure, &attempt_context);
                    if matches!(failure, AttemptFailure::Infrastructure(_)) {
                        let detail = failure
                            .execution_error()
                            .map(|error| error.message().to_owned())
                            .unwrap_or_else(|| "async timer failed".to_owned());
                        return Err(self.finish_with_execution_error(
                            Some(failure),
                            attempt_context,
                            &detail,
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
                        .budget_reason(
                            attempts,
                            operation_elapsed,
                            total_elapsed,
                        )
                        .or_else(|| self.flow_timeout_reason(total_elapsed))
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
                    let scheduled =
                        attempt_context.with_next_delay(step.effective_delay());
                    self.retry.observers().retry_scheduled(&step, &scheduled);
                    let after_observer = elapsed_since(clock, started_at);
                    if let Some(reason) = self
                        .budget_reason(
                            attempts,
                            operation_elapsed,
                            after_observer,
                        )
                        .or_else(|| self.flow_timeout_reason(after_observer))
                    {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            scheduled,
                        ));
                    }
                    if let Err(error) =
                        sleep(&timer, step.effective_delay()).await
                    {
                        return Err(self.finish_with_timer_error(
                            Some(failure),
                            scheduled,
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

    fn budget_reason(
        &self,
        attempts: u32,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> Option<RetryErrorReason> {
        let limits = self.retry.policy().limits();
        if attempts >= limits.max_attempts().get() {
            Some(RetryErrorReason::AttemptsExhausted)
        } else if limits
            .max_operation_elapsed()
            .is_some_and(|limit| operation_elapsed >= limit)
        {
            Some(RetryErrorReason::OperationBudgetExhausted)
        } else if limits
            .max_total_elapsed()
            .is_some_and(|limit| total_elapsed >= limit)
        {
            Some(RetryErrorReason::TotalBudgetExhausted)
        } else {
            None
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
        self.finish_with_execution_error(failure, context, &error.to_string())
    }

    fn finish_with_execution_error(
        &self,
        failure: Option<AttemptFailure<E>>,
        context: RetryContext,
        message: &str,
    ) -> RetryError<E> {
        let error = RetryError::new_with_execution_error(
            RetryErrorReason::TimerFailed,
            failure,
            crate::RetryExecutionError::timer(message),
            context,
        );
        self.retry
            .observers()
            .finished(RetryOutcomeKind::Failed, error.context());
        error
    }
}

async fn execute_attempt<T, E, F>(
    timer: &Arc<dyn Timer>,
    timeout: Option<Duration>,
    attempt_timeout: Option<Duration>,
    operation: F,
) -> Result<T, AttemptFailure<E>>
where
    F: Future<Output = Result<T, E>>,
{
    let Some(timeout) = timeout else {
        return operation.await.map_err(AttemptFailure::Error);
    };
    let mut timer_future = match timer.after(timeout) {
        Ok(timer_future) => timer_future,
        Err(error) => {
            return Err(AttemptFailure::Infrastructure(
                crate::AttemptExecutionError::new(&error.to_string()),
            ));
        }
    };
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => match result {
            Ok(value) => Ok(value),
            Err(error) => Err(AttemptFailure::Error(error)),
        },
        result = &mut timer_future => {
            let kind = if attempt_timeout.is_some_and(|limit| timeout <= limit) {
                AttemptTimeoutKind::Attempt
            } else {
                AttemptTimeoutKind::Flow
            };
            match result {
                Ok(()) => Err(AttemptFailure::Timeout { kind }),
                Err(error) => Err(AttemptFailure::Infrastructure(
                    crate::AttemptExecutionError::new(&error.to_string()),
                )),
            }
        }
    }
}

async fn sleep(
    timer: &Arc<dyn Timer>,
    delay: Duration,
) -> Result<(), TimeError> {
    let future = timer.after(delay)?;
    future.await
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

fn elapsed_since(
    clock: &dyn MonotonicClock,
    started: MonotonicInstant,
) -> Duration {
    clock
        .now()
        .duration_since(started)
        .expect("retry clock must be monotonic")
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
