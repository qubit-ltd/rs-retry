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

use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_clock::TokioTimer;

use super::internal::AsyncAttemptOutcome;
use super::internal::AsyncBackoffOutcome;
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
///
/// # Type Parameters
/// - `'a`: Lifetime of the borrowed retry definition.
/// - `E`: Application error; operation futures need not be Send.
///
/// # Examples
///
/// ```
/// use std::future;
/// use std::time::Duration;
///
/// use qubit_retry::AsyncRetry;
/// use qubit_retry::Retry;
/// use qubit_retry::RetryCancellationPhase;
/// use qubit_retry::RetryCancellationToken;
/// use qubit_retry::RetryFailure;
/// use qubit_retry::RetryPolicy;
///
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     let retry = Retry::<&str>::builder(RetryPolicy::builder().build().unwrap()).build();
///     let token = RetryCancellationToken::new();
///     let operation_token = token.clone();
///     let execution: AsyncRetry<'_, &str> = retry.asynchronous();
///     let error = execution
///         .attempt_timeout(Duration::from_secs(2))
///         .flow_timeout(Duration::from_secs(5))
///         .cancellation_token(token)
///         .run(move || {
///             operation_token.cancel();
///             future::pending::<Result<(), &str>>()
///         }).await.unwrap_err();
///     assert!(matches!(error.failure(), RetryFailure::Cancelled {
///         phase: RetryCancellationPhase::Attempt, ..
///     }));
/// }
/// ```
#[must_use]
pub struct AsyncRetry<'a, E> {
    /// Borrowed immutable policy and callbacks.
    retry: &'a Retry<E>,
    /// Optional hard limit for each admitted attempt.
    attempt_timeout: Option<Duration>,
    /// Optional hard limit measured from execution start.
    flow_timeout: Option<Duration>,
    /// Optional shared cancellation source; None disables external
    /// cancellation.
    cancellation_token: Option<RetryCancellationToken>,
    /// Timer and monotonic clock used by this execution.
    timer: Option<Arc<dyn Timer>>,
    /// Shared random source for uniform delays and jitter.
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> AsyncRetry<'a, E> {
    ///
    /// Creates a facade borrowing the immutable retry definition.
    ///
    /// # Parameters
    /// - `retry`: Definition that must outlive this facade.
    ///
    /// # Returns
    /// An execution facade with default runtime controls.
    #[inline]
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
    ///
    /// # Parameters
    /// - `timeout`: Duration measured by the configured monotonic clock; zero
    ///   prevents admission.
    ///
    /// # Returns
    /// This facade with the selected hard timeout enabled.
    #[inline(always)]
    pub fn attempt_timeout(mut self, timeout: Duration) -> Self {
        self.attempt_timeout = Some(timeout);
        self
    }

    /// Sets the wall-clock timeout for the entire flow.
    ///
    /// # Parameters
    /// - `timeout`: Duration measured by the configured monotonic clock; zero
    ///   prevents admission.
    ///
    /// # Returns
    /// This facade with the selected hard timeout enabled.
    #[inline(always)]
    pub fn flow_timeout(mut self, timeout: Duration) -> Self {
        self.flow_timeout = Some(timeout);
        self
    }

    /// Sets the cooperative cancellation token observed by this execution.
    ///
    /// # Parameters
    /// - `token`: Cancellation source shared with the caller.
    ///
    /// # Returns
    /// This facade observing the supplied source.
    #[inline(always)]
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Injects a timer and clock, primarily for deterministic tests.
    ///
    /// # Parameters
    /// - `timer`: Shared timer and monotonic clock.
    ///
    /// # Returns
    /// This facade using the supplied runtime resource.
    #[inline(always)]
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.timer = Some(timer);
        self
    }

    /// Injects the random source used by uniform backoff delays and jitter.
    ///
    /// # Parameters
    /// - `random_source`: Shared sampler for uniform delays and jitter.
    ///
    /// # Returns
    /// This facade using the supplied runtime resource.
    #[inline(always)]
    pub fn random_source(mut self, random_source: Arc<dyn RetryRandomSource>) -> Self {
        self.random_source = random_source;
        self
    }

    /// Executes one future per attempt.
    ///
    /// Completion observers run synchronously with the frozen result; their
    /// panics are attached as diagnostics and do not change the outcome.
    /// Dropping this future or an operation panic does not guarantee completion
    /// notification. The operation future need not be `Send` or static.
    ///
    /// # Type Parameters
    /// - `T`: Successful value returned to the caller.
    /// - `F`: Operation factory invoked once per admitted attempt.
    /// - `Fut`: Future produced for one attempt; need not be Send.
    ///
    /// # Parameters
    /// - `operation`: Operation whose errors are classified by the registered
    ///   rules.
    ///
    /// # Returns
    /// The successful value with its frozen context and completion diagnostics.
    ///
    /// # Errors
    /// Returns the terminal attempt, cancellation, timeout, budget, callback,
    /// or infrastructure failure with its context.
    ///
    /// # Panics
    /// Operation panics unwind through the caller; custom timer, random source,
    /// or clock panics are not intercepted.
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
    #[inline(always)]
    pub async fn run<T, F, Fut>(&self, operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        self.retry.complete(self.run_inner(operation).await)
    }

    /// Executes retry controls and freezes the final result before completion
    /// observers run. Returns the original terminal error on control failure.
    ///
    /// # Type Parameters
    /// - `T`: Successful value returned to the caller.
    /// - `F`: Operation factory invoked once per admitted attempt.
    /// - `Fut`: Future produced for one attempt; need not be Send.
    ///
    /// # Parameters
    /// - `operation`: Operation whose errors are classified by the registered
    ///   rules.
    ///
    /// # Returns
    /// The successful value with its frozen context and initially empty
    /// diagnostics.
    ///
    /// # Errors
    /// Returns the terminal attempt, cancellation, timeout, budget, callback,
    /// or infrastructure failure with its context.
    ///
    /// # Panics
    /// Operation panics unwind through the caller; custom timer, random source,
    /// or clock panics are not intercepted.
    #[allow(
        clippy::result_large_err,
        reason = "the internal helper propagates the lossless public terminal error"
    )]
    async fn run_inner<T, F, Fut>(&self, mut operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let timer = self.timer.clone().unwrap_or_else(|| Arc::new(TokioTimer::current()));
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
            let admission_sample = controller.before_attempt(clock, cancellation)?;
            let plan = controller.prepare_attempt(admission_sample)?;
            let timeout_future = match register_timeout(&timer, plan.deadline()) {
                Ok(timeout_future) => timeout_future,
                Err(error) => {
                    return Err(controller.record_inactive_infrastructure_failure(timer_failure(error), clock.now()));
                }
            };
            controller.commit_prepared_attempt(plan, clock, cancellation)?;
            let outcome = execute_attempt(timeout_future, plan.scope(), cancellation, operation()).await;

            let directive = match outcome {
                AsyncAttemptOutcome::Completed(Ok(value)) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(value, context));
                }
                AsyncAttemptOutcome::Completed(Err(error)) => {
                    controller.record_failure(AttemptFailure::Error(error), clock, cancellation)?
                }
                AsyncAttemptOutcome::TimedOut(scope) => {
                    controller.record_failure(AttemptFailure::TimedOut { scope }, clock, cancellation)?
                }
                AsyncAttemptOutcome::Cancelled => {
                    return Err(controller.record_attempt_cancellation(clock));
                }
                AsyncAttemptOutcome::TimerFailed(error) => {
                    let error = controller.record_active_infrastructure_failure(timer_failure(error), clock.now());
                    return Err(error);
                }
            };
            match sleep(&timer, directive.deadline(), cancellation).await {
                AsyncBackoffOutcome::Elapsed => {}
                AsyncBackoffOutcome::Cancelled => {
                    return Err(controller.record_backoff_cancellation(clock));
                }
                AsyncBackoffOutcome::TimerFailed(error) => {
                    let error = controller.record_inactive_infrastructure_failure(timer_failure(error), clock.now());
                    return Err(error);
                }
            }
        }
    }
}

/// Polls one operation with its optional cooperative timeout.
///
/// # Type Parameters
/// - `T`: Successful operation value.
/// - `E`: Application error.
/// - `F`: Future for this single attempt.
///
/// # Parameters
/// - `timeout_future`: Registered timeout, or None for no hard deadline.
/// - `timeout_scope`: Source of the registered deadline, or None without a
///   timer.
/// - `cancellation`: Optional shared flow cancellation source.
/// - `operation`: Future polled before cancellation and timeout on each select
///   cycle.
///
/// # Returns
/// The first selected outcome; a ready operation wins a same-poll tie.
///
/// # Panics
/// Panics if a registered timer lacks its scope, indicating an internal
/// invariant failure. Operation panics propagate.
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
            let timeout_scope = timeout_scope.expect("a registered attempt timeout always retains its scope");
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
            let timeout_scope = timeout_scope.expect("a registered attempt timeout always retains its scope");
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
///
/// # Parameters
/// - `timer`: Timer belonging to the current flow clock.
/// - `deadline`: Absolute deadline, or None to disable the timer.
///
/// # Returns
/// Some registered future, or None when no deadline was supplied.
#[inline]
fn register_timeout(
    timer: &Arc<dyn Timer>,
    deadline: Option<MonotonicInstant>,
) -> Result<Option<TimerFuture>, TimeError> {
    deadline.map(|deadline| timer.at(deadline)).transpose()
}

/// Waits for one retry delay using the configured timer and cancellation token.
///
/// # Parameters
/// - `timer`: Timer used for the selected delay.
/// - `deadline`: Absolute backoff deadline.
/// - `cancellation`: Optional cancellation source.
///
/// # Returns
/// Elapsed, cancelled, or timer-failed status; cancellation wins same-poll
/// readiness.
async fn sleep(
    timer: &Arc<dyn Timer>,
    deadline: MonotonicInstant,
    cancellation: Option<&RetryCancellationToken>,
) -> AsyncBackoffOutcome {
    if cancellation.is_some_and(RetryCancellationToken::is_cancelled) {
        return AsyncBackoffOutcome::Cancelled;
    }
    let mut timer_future = match timer.at(deadline) {
        Ok(future) => future,
        Err(_) if cancellation.is_some_and(RetryCancellationToken::is_cancelled) => {
            return AsyncBackoffOutcome::Cancelled;
        }
        Err(error) => return AsyncBackoffOutcome::TimerFailed(error),
    };
    let Some(token) = cancellation else {
        return match timer_future.await {
            Ok(()) => AsyncBackoffOutcome::Elapsed,
            Err(error) => AsyncBackoffOutcome::TimerFailed(error),
        };
    };
    let cancellation = token.cancelled();
    tokio::pin!(cancellation);
    tokio::select! {
        biased;
        () = &mut cancellation => AsyncBackoffOutcome::Cancelled,
        result = &mut timer_future => match result {
            Ok(()) => AsyncBackoffOutcome::Elapsed,
            Err(error) => AsyncBackoffOutcome::TimerFailed(error),
        },
    }
}

/// Converts one timer error into the public infrastructure failure model.
///
/// # Parameters
/// - `error`: Timer error whose display text is retained.
///
/// # Returns
/// A structured timer infrastructure failure.
#[inline]
fn timer_failure(error: TimeError) -> RetryInfrastructureFailure {
    RetryInfrastructureFailure::Timer {
        message: error.to_string().into_boxed_str(),
    }
}
