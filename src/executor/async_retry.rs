// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tokio execution facade for the policy-based retry API.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_clock::TokioTimer;

use super::internal::RetryFlowController;
use super::retry::Retry;
use crate::AttemptFailure;
use crate::RetryCancellationToken;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::RetryTimeoutScope;
use crate::random::ThreadRetryRandomSource;

/// Tokio retry execution with explicit attempt and flow timeout controls.
pub struct AsyncRetry<'a, E> {
    retry: &'a Retry<E>,
    attempt_timeout: Option<Duration>,
    flow_timeout: Option<Duration>,
    cancellation_token: Option<RetryCancellationToken>,
    timer: Option<Arc<dyn Timer>>,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> AsyncRetry<'a, E> {
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            attempt_timeout: None,
            flow_timeout: None,
            cancellation_token: None,
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

    /// Sets the cooperative cancellation token observed by this execution.
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
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
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
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
        let mut controller = RetryFlowController::new(
            clock.now(),
            self.retry,
            Arc::clone(&self.random_source),
            self.attempt_timeout,
            self.flow_timeout,
        );

        loop {
            let cancellation = self.cancellation_token.as_ref();
            let admission_sample =
                controller.before_attempt(clock, cancellation)?;
            let plan = controller.prepare_async_attempt(admission_sample)?;
            let timeout_future = match register_timeout(&timer, plan.deadline())
            {
                Ok(timeout_future) => timeout_future,
                Err(error) => {
                    return Err(controller
                        .record_inactive_infrastructure_failure(
                            timer_failure(error),
                            clock.now(),
                        ));
                }
            };
            controller.commit_prepared_attempt(plan, clock, cancellation)?;
            let outcome = execute_attempt(
                timeout_future,
                plan.scope(),
                cancellation,
                operation(),
            )
            .await;

            let directive = match outcome {
                AsyncAttemptOutcome::Completed(Ok(value)) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(value, context));
                }
                AsyncAttemptOutcome::Completed(Err(error)) => controller
                    .record_failure(
                        AttemptFailure::Error(error),
                        clock,
                        cancellation,
                    )?,
                AsyncAttemptOutcome::TimedOut(scope) => controller
                    .record_failure(
                        AttemptFailure::TimedOut { scope },
                        clock,
                        cancellation,
                    )?,
                AsyncAttemptOutcome::Cancelled => {
                    return Err(controller.record_attempt_cancellation(clock));
                }
                AsyncAttemptOutcome::TimerFailed(error) => {
                    let error = controller
                        .record_active_infrastructure_failure(
                            timer_failure(error),
                            clock.now(),
                        );
                    return Err(error);
                }
            };
            match sleep(&timer, directive.sleep_duration(), cancellation).await
            {
                BackoffOutcome::Elapsed => {}
                BackoffOutcome::Cancelled => {
                    return Err(controller.record_backoff_cancellation(clock));
                }
                BackoffOutcome::TimerFailed(error) => {
                    let error = controller
                        .record_inactive_infrastructure_failure(
                            timer_failure(error),
                            clock.now(),
                        );
                    return Err(error);
                }
            }
        }
    }
}

/// Result of running an async attempt and its cooperative timeout timer.
enum AsyncAttemptOutcome<T, E> {
    /// The operation future completed before its timeout.
    Completed(Result<T, E>),
    /// The cooperative timeout completed first.
    TimedOut(RetryTimeoutScope),
    /// Cooperative cancellation interrupted the operation future.
    Cancelled,
    /// Registering or polling the timeout timer failed.
    TimerFailed(TimeError),
}

/// Polls one operation with its optional cooperative timeout.
async fn execute_attempt<T, E, F>(
    timeout_future: Option<TimerFuture>,
    timeout_scope: Option<RetryTimeoutScope>,
    cancellation: Option<&RetryCancellationToken>,
    operation: F,
) -> AsyncAttemptOutcome<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    tokio::pin!(operation);
    match (timeout_future, cancellation) {
        (Some(mut timer_future), Some(token)) => {
            let cancellation = token.cancelled();
            tokio::pin!(cancellation);
            let timeout_scope = timeout_scope.expect(
                "a registered attempt timeout always retains its scope",
            );
            tokio::select! {
                biased;
                result = &mut operation => AsyncAttemptOutcome::Completed(result),
                () = &mut cancellation => AsyncAttemptOutcome::Cancelled,
                result = &mut timer_future => match result {
                    Ok(()) => AsyncAttemptOutcome::TimedOut(timeout_scope),
                    Err(error) => AsyncAttemptOutcome::TimerFailed(error),
                },
            }
        }
        (Some(mut timer_future), None) => {
            let timeout_scope = timeout_scope.expect(
                "a registered attempt timeout always retains its scope",
            );
            tokio::select! {
                biased;
                result = &mut operation => AsyncAttemptOutcome::Completed(result),
                result = &mut timer_future => match result {
                    Ok(()) => AsyncAttemptOutcome::TimedOut(timeout_scope),
                    Err(error) => AsyncAttemptOutcome::TimerFailed(error),
                },
            }
        }
        (None, Some(token)) => {
            let cancellation = token.cancelled();
            tokio::pin!(cancellation);
            tokio::select! {
                biased;
                result = &mut operation => AsyncAttemptOutcome::Completed(result),
                () = &mut cancellation => AsyncAttemptOutcome::Cancelled,
            }
        }
        (None, None) => AsyncAttemptOutcome::Completed(operation.await),
    }
}

/// Registers the optional absolute deadline before counting the attempt.
///
/// # Errors
/// Returns the timer's registration error without polling an operation future.
fn register_timeout(
    timer: &Arc<dyn Timer>,
    deadline: Option<qubit_clock::MonotonicInstant>,
) -> Result<Option<TimerFuture>, TimeError> {
    deadline.map(|deadline| timer.at(deadline)).transpose()
}

/// Result of waiting for one retry delay.
enum BackoffOutcome {
    /// The delay elapsed normally.
    Elapsed,
    /// Cooperative cancellation interrupted the delay.
    Cancelled,
    /// Registering or polling the delay timer failed.
    TimerFailed(TimeError),
}

/// Waits for one retry delay using the configured timer and cancellation token.
async fn sleep(
    timer: &Arc<dyn Timer>,
    delay: Duration,
    cancellation: Option<&RetryCancellationToken>,
) -> BackoffOutcome {
    if cancellation.is_some_and(RetryCancellationToken::is_cancelled) {
        return BackoffOutcome::Cancelled;
    }
    let mut timer_future = match timer.after(delay) {
        Ok(future) => future,
        Err(_)
            if cancellation
                .is_some_and(RetryCancellationToken::is_cancelled) =>
        {
            return BackoffOutcome::Cancelled;
        }
        Err(error) => return BackoffOutcome::TimerFailed(error),
    };
    let Some(token) = cancellation else {
        return match timer_future.await {
            Ok(()) => BackoffOutcome::Elapsed,
            Err(error) => BackoffOutcome::TimerFailed(error),
        };
    };
    let cancellation = token.cancelled();
    tokio::pin!(cancellation);
    tokio::select! {
        biased;
        () = &mut cancellation => BackoffOutcome::Cancelled,
        result = &mut timer_future => match result {
            Ok(()) => BackoffOutcome::Elapsed,
            Err(error) => BackoffOutcome::TimerFailed(error),
        },
    }
}

/// Converts one timer error into the public infrastructure failure model.
fn timer_failure(error: TimeError) -> RetryInfrastructureFailure {
    RetryInfrastructureFailure::Timer {
        message: error.to_string().into_boxed_str(),
    }
}
