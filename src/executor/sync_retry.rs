// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Same-thread execution facade for the pure retry policy API.

use std::sync::Arc;

use qubit_clock::StdTimer;
use qubit_clock::Timer;

use super::internal::BlockingBackoffOutcome;
use super::internal::RetryFlowController;
use super::internal::wait_for_backoff;
use super::retry::Retry;
use super::retry_cancellation_token::RetryCancellationToken;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;

/// Same-thread retry execution. It intentionally exposes no timeout method.
///
/// # Type Parameters
/// - `'a`: Lifetime of the borrowed retry definition.
/// - `E`: Application error; synchronous operations may capture non-Send state.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryPolicy;
/// use qubit_retry::executor::SyncRetry;
///
/// let retry = Retry::<&str>::builder(RetryPolicy::builder().build()?).build();
/// let execution: SyncRetry<'_, &str> = retry.sync();
/// let value = execution.run(|| Ok(7)).expect("operation succeeds");
/// assert_eq!(*value.value(), 7);
/// assert_eq!(value.context().attempts(), 1);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
pub struct SyncRetry<'a, E> {
    /// Borrowed immutable policy and callbacks.
    retry: &'a Retry<E>,
    /// Optional shared cancellation source; None disables external
    /// cancellation.
    cancellation_token: Option<RetryCancellationToken>,
    /// Timer and monotonic clock used by this execution.
    timer: Option<Arc<dyn Timer>>,
    /// Shared random source for uniform delays and jitter.
    random_source: Option<Arc<dyn RetryRandomSource>>,
}

impl<'a, E: 'static> SyncRetry<'a, E> {
    /// Creates a synchronous facade from one retry policy.
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
            cancellation_token: None,
            timer: None,
            random_source: None,
        }
    }

    /// Sets the token used to cancel this synchronous retry flow.
    ///
    /// Cancellation is observed before an operation starts, after a failed
    /// operation, and while waiting for backoff. It cannot interrupt an
    /// operation that is already running on the calling thread.
    ///
    /// # Parameters
    ///
    /// - `token`: Shared flow cancellation token.
    ///
    /// # Returns
    ///
    /// A synchronous facade that observes the supplied token.
    #[inline(always)]
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Replaces the blocking timer used by this execution.
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

    /// Replaces the random source used by this execution.
    ///
    /// # Parameters
    /// - `random`: Shared sampler for uniform delays and jitter.
    ///
    /// # Returns
    /// This facade using the supplied runtime resource.
    #[inline(always)]
    pub fn random_source(mut self, random: Arc<dyn RetryRandomSource>) -> Self {
        self.random_source = Some(random);
        self
    }

    /// Runs a same-thread operation until success or a terminal retry error.
    ///
    /// Completion observers run synchronously with the frozen result; their
    /// panics are attached as diagnostics and do not change the outcome.
    /// Operation panics propagate without completion notification.
    ///
    /// # Type Parameters
    /// - `T`: Successful value returned to the caller.
    /// - `F`: Operation invoked once per admitted attempt.
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
    pub fn run<T, F>(&self, operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        self.retry.complete(self.run_inner(operation))
    }

    /// Executes retry controls and freezes the final result before completion
    /// observers run. Returns the original terminal error on control failure.
    ///
    /// # Type Parameters
    /// - `T`: Successful value returned to the caller.
    /// - `F`: Operation invoked once per admitted attempt.
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
    fn run_inner<T, F>(&self, mut operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let default_timer = StdTimer::new();
        let timer: &dyn Timer = self.timer.as_deref().unwrap_or(&default_timer);
        let clock = timer.clock();
        let mut controller = RetryFlowController::new(clock.now(), self.retry, self.random_source.clone(), None, None);

        loop {
            let cancellation = self.cancellation_token.as_ref();
            let _ = controller.before_attempt(clock, cancellation)?;
            controller.commit_attempt(clock, cancellation)?;
            let result = operation();

            match result {
                Ok(value) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(value, context));
                }
                Err(error) => {
                    let directive = controller.record_failure(AttemptFailure::Error(error), clock, cancellation)?;
                    match wait_for_backoff(timer, directive.deadline(), directive.is_immediate(), cancellation) {
                        BlockingBackoffOutcome::Elapsed => {}
                        BlockingBackoffOutcome::Cancelled => {
                            return Err(controller.record_backoff_cancellation(clock));
                        }
                        BlockingBackoffOutcome::TimerFailed(timer_error) => {
                            let error = controller.record_inactive_infrastructure_failure(
                                RetryInfrastructureFailure::Timer {
                                    message: timer_error.to_string().into_boxed_str(),
                                },
                                clock.now(),
                            );
                            return Err(error);
                        }
                    }
                }
            }
        }
    }
}
