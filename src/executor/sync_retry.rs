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
use crate::random::ThreadRetryRandomSource;

/// Same-thread retry execution. It intentionally exposes no timeout method.
pub struct SyncRetry<'a, E> {
    retry: &'a Retry<E>,
    cancellation_token: Option<RetryCancellationToken>,
    timer: Arc<dyn Timer>,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> SyncRetry<'a, E> {
    /// Creates a synchronous facade from one retry policy.
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            cancellation_token: None,
            timer: Arc::new(StdTimer::new()),
            random_source: Arc::new(ThreadRetryRandomSource),
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
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Replaces the blocking timer used by this execution.
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.timer = timer;
        self
    }

    /// Replaces the random source used by this execution.
    pub fn random_source(mut self, random: Arc<dyn RetryRandomSource>) -> Self {
        self.random_source = random;
        self
    }

    /// Runs a same-thread operation until success or a terminal retry error.
    ///
    /// Completion observers run synchronously with the frozen result; their
    /// panics are attached as diagnostics and do not change the outcome.
    /// Operation panics propagate without completion notification.
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
    pub fn run<T, F>(
        &self,
        operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let mut result = self.run_inner(operation);
        let failures = match &result {
            Ok(success) => {
                self.retry.observers().notify_success(success.context())
            }
            Err(error) => self
                .retry
                .observers()
                .notify_terminal_failure(error.failure(), error.context()),
        };
        match &mut result {
            Ok(success) => success.set_completion_callback_failures(failures),
            Err(error) => error.set_completion_callback_failures(failures),
        }
        result
    }

    /// Executes retry controls and freezes the final result before completion
    /// observers run. Returns the original terminal error on control failure.
    #[allow(
        clippy::result_large_err,
        reason = "the internal helper propagates the lossless public terminal error"
    )]
    fn run_inner<T, F>(
        &self,
        mut operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let clock = self.timer.clock();
        let mut controller =
            RetryFlowController::new(clock.now(), self.retry, Arc::clone(&self.random_source), None, None);

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
                    let directive =
                        controller.record_failure(AttemptFailure::Error(error), clock, cancellation)?;
                    match wait_for_backoff(&self.timer, directive.sleep_duration(), cancellation) {
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
