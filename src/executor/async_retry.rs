// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Tokio execution facade for the policy-based retry API.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TokioTimer;

use super::internal::EffectiveTimeout;
use super::internal::RetryFlowState;
use super::retry::Retry;
use crate::AttemptFailure;
use crate::AttemptTimeoutKind;
use crate::RetryContext;
use crate::RetryError;
use crate::RetryErrorReason;
use crate::RetryRandomSource;
use crate::RetrySuccess;
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
        let mut flow = RetryFlowState::new(
            clock,
            self.retry.policy(),
            Arc::clone(&self.random_source),
            self.flow_timeout,
        )
        .expect("validated retry limits must fit the monotonic clock");
        let mut last_failure = None;

        loop {
            let snapshot = flow.snapshot();
            if let Some(reason) = flow.continuation_reason() {
                return Err(self.finish(
                    reason,
                    last_failure,
                    flow.context(snapshot, snapshot.attempts()),
                ));
            }

            let before = flow.upcoming_context();
            self.retry.observers().attempt_started(&before);

            let attempt = match flow.begin_attempt() {
                Ok(attempt) => attempt,
                Err(reason) => {
                    return Err(self.finish(
                        reason,
                        last_failure,
                        flow.current_context(),
                    ));
                }
            };
            let timeout = flow.effective_timeout(self.attempt_timeout);
            let outcome = execute_attempt(&timer, timeout, operation()).await;
            let snapshot = flow.finish_attempt(attempt);
            let attempt_context = flow
                .context(snapshot, snapshot.attempts())
                .with_attempt_timeout(timeout.map(EffectiveTimeout::duration));

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
                    if let Some(reason) = flow.continuation_reason() {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    let step = flow.next_backoff(decision);
                    let scheduled =
                        attempt_context.with_next_delay(step.effective_delay());
                    self.retry.observers().retry_scheduled(&step, &scheduled);
                    if let Some(reason) =
                        flow.retry_reason(step.effective_delay())
                    {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            scheduled,
                        ));
                    }
                    if let Some(remaining) =
                        flow.flow_sleep_cap(step.effective_delay())
                    {
                        if let Err(error) = sleep(&timer, remaining).await {
                            return Err(self.finish_with_timer_error(
                                Some(failure),
                                scheduled,
                                error,
                            ));
                        }
                        return Err(self.finish(
                            RetryErrorReason::FlowTimedOut,
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
    timeout: Option<EffectiveTimeout>,
    operation: F,
) -> Result<T, AttemptFailure<E>>
where
    F: Future<Output = Result<T, E>>,
{
    let Some(timeout) = timeout else {
        return operation.await.map_err(AttemptFailure::Error);
    };
    let mut timer_future = match timer.after(timeout.duration()) {
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
            match result {
                Ok(()) => Err(AttemptFailure::Timeout {
                    kind: timeout.kind(),
                }),
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
