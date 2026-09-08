// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Worker-thread execution facade for blocking operations.

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;
use qubit_clock::StdTimer;
use qubit_clock::Timer;

use super::attempt_cancellation_token::AttemptCancellationToken;
use super::internal::BlockingBackoffOutcome;
use super::internal::RetryFlowController;
use super::internal::WorkerAttemptExecutor;
use super::internal::wait_for_backoff;
use super::retry_cancellation_token::RetryCancellationToken;
use super::retry_config::RetryConfig;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;
use crate::executor::internal::BlockingAttempt;
use crate::executor::internal::BlockingAttemptOutcome;
use crate::executor::internal::BlockingValueOperation;

/// Worker retry execution with cooperative cancellation.
///
/// # Type Parameters
/// - `'a`: Lifetime of the borrowed retry definition.
/// - `E`: Owned application error transferred from the worker thread; execution
///   requires Send and static.
///
/// # Examples
///
/// ```
/// use qubit_retry::RetryConfig;
/// use qubit_retry::WorkerRetry;
///
/// let config = RetryConfig::<&str>::builder().max_attempts(3).build()?;
/// let value = WorkerRetry::new(&config).run(|token| {
///     assert!(!token.is_cancelled());
///     Ok(7)
/// }).expect("worker returns and exits");
/// assert_eq!(*value.value(), 7);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
pub struct WorkerRetry<'a, E> {
    /// Borrowed immutable policy and callbacks.
    config: &'a RetryConfig<E>,
    /// OS-visible name assigned to each attempt thread.
    thread_name: Box<str>,
    /// Optional requested stack size; None uses the OS default.
    stack_size: Option<usize>,
    /// Optional hard limit for each admitted attempt.
    attempt_timeout: Option<Duration>,
    /// Optional hard limit measured from execution start.
    flow_timeout: Option<Duration>,
    /// Real-time bound for joining a worker after requesting its cancellation.
    cancellation_grace: Duration,
    /// Optional shared cancellation source; None disables external
    /// cancellation.
    cancellation_token: Option<RetryCancellationToken>,
    /// Timer and monotonic clock used by this execution.
    timer: Option<Arc<dyn Timer>>,
    /// Shared random source for uniform delays and jitter.
    random_source: Option<Arc<dyn RetryRandomSource>>,
}

impl<'a, E: Send + 'static> WorkerRetry<'a, E> {
    ///
    /// Creates a facade borrowing the immutable retry definition.
    ///
    /// # Parameters
    /// - `retry`: Definition that must outlive this facade.
    ///
    /// # Returns
    /// An execution facade with default runtime controls.
    #[inline]
    pub fn new(config: &'a RetryConfig<E>) -> Self {
        Self {
            config,
            thread_name: "qubit-retry-worker".into(),
            stack_size: None,
            attempt_timeout: None,
            flow_timeout: None,
            cancellation_grace: Duration::from_millis(100),
            cancellation_token: None,
            timer: None,
            random_source: None,
        }
    }

    /// Sets the maximum duration of one worker attempt.
    ///
    /// # Parameters
    /// - `timeout`: Duration measured by the configured monotonic clock; zero
    ///   prevents admission.
    ///
    /// # Returns
    /// This facade with the selected hard timeout enabled.
    #[inline(always)]
    pub fn hard_attempt_timeout(mut self, timeout: Duration) -> Self {
        self.attempt_timeout = Some(timeout);
        self
    }

    /// Sets the wall-clock timeout for the complete flow.
    ///
    /// # Parameters
    /// - `timeout`: Duration measured by the configured monotonic clock; zero
    ///   prevents admission.
    ///
    /// # Returns
    /// This facade with the selected hard timeout enabled.
    #[inline(always)]
    pub fn hard_flow_timeout(mut self, timeout: Duration) -> Self {
        self.flow_timeout = Some(timeout);
        self
    }

    /// Sets the real-time grace period used after requesting cooperative
    /// cancellation. This OS-thread cleanup bound always uses standard wall
    /// duration, even when attempts and backoff use an injected manual
    /// timer. It prevents a stopped virtual clock from retaining an
    /// uncooperative worker indefinitely.
    ///
    /// # Parameters
    /// - `grace`: Maximum real-time cleanup wait; zero performs only an
    ///   immediate join check.
    ///
    /// # Returns
    /// This facade with the supplied cleanup bound.
    #[inline(always)]
    pub fn cancellation_grace(mut self, grace: Duration) -> Self {
        self.cancellation_grace = grace;
        self
    }

    /// Sets the token used to cancel the complete worker retry flow.
    ///
    /// # Parameters
    /// - `token`: Shared flow cancellation token.
    ///
    /// # Returns
    /// A worker facade that observes the supplied token.
    #[inline(always)]
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Sets the OS-visible name assigned to each worker thread.
    ///
    /// # Parameters
    /// - `name`: Name passed to [`std::thread::Builder`].
    ///
    /// # Returns
    /// A worker facade using the supplied thread name.
    #[inline(always)]
    pub fn thread_name(mut self, name: &str) -> Self {
        self.thread_name = name.into();
        self
    }

    /// Sets the stack size requested for each worker thread.
    ///
    /// # Parameters
    /// - `stack_size`: Requested worker stack size in bytes.
    ///
    /// # Returns
    /// A worker facade using the supplied stack size.
    #[inline(always)]
    pub fn worker_stack_size(mut self, stack_size: usize) -> Self {
        self.stack_size = Some(stack_size);
        self
    }

    /// Injects the timer for attempt/flow deadlines, budget accounting and
    /// backoff. Cancellation grace remains a real-time OS-thread cleanup
    /// bound. The timer must progress independently while this synchronous
    /// facade blocks.
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
        self.random_source = Some(random_source);
        self
    }

    /// Runs one cancellable worker operation per attempt.
    ///
    /// Completion observers run synchronously with the frozen result; their
    /// panics are attached as diagnostics and do not change the outcome.
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
    /// Custom timer, random source, or clock panics are not intercepted. Worker
    /// operation panics become structured attempt failures.
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
    #[inline(always)]
    pub fn run<T, F>(&self, operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        T: Send + 'static,
        F: Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static,
    {
        self.config.complete(self.run_inner(operation))
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
    /// Custom timer, random source, or clock panics are not intercepted. Worker
    /// operation panics become structured attempt failures.
    #[allow(
        clippy::result_large_err,
        reason = "the internal helper propagates the lossless public terminal error"
    )]
    fn run_inner<T, F>(&self, operation: F) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        T: Send + 'static,
        F: Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let operation = Arc::new(BlockingValueOperation::new(operation));
        let worker_operation: Arc<dyn BlockingAttempt<E>> = operation.clone();
        let default_timer = StdTimer::new();
        let timer: &dyn Timer = self.timer.as_deref().unwrap_or(&default_timer);
        let clock = timer.clock();
        let mut controller = RetryFlowController::new(
            clock.now(),
            self.config,
            self.random_source.clone(),
            self.attempt_timeout,
            self.flow_timeout,
        );

        loop {
            let cancellation = self.cancellation_token.as_ref();
            let admission_sample = controller.before_attempt(clock, cancellation)?;
            let plan = controller.prepare_attempt(admission_sample)?;
            let timeout_future = match plan.deadline().map(|deadline| timer.at(deadline)).transpose() {
                Ok(future) => future,
                Err(error) => {
                    return Err(controller.record_inactive_infrastructure_failure(
                        RetryInfrastructureFailure::Timer {
                            message: error.to_string().into_boxed_str(),
                        },
                        clock.now(),
                    ));
                }
            };
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&worker_operation),
                &self.thread_name,
                self.stack_size,
                self.cancellation_grace,
                cancellation,
                timeout_future.map(|future| (plan.scope().expect("registered timeout retains its scope"), future)),
                || controller.commit_prepared_attempt(plan, clock, cancellation),
            )?;

            match outcome {
                BlockingAttemptOutcome::Completed(Ok(())) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(operation.take_value(), context));
                }
                BlockingAttemptOutcome::TimerFailed { error } => {
                    return Err(controller.record_inactive_infrastructure_failure(
                        RetryInfrastructureFailure::Timer {
                            message: error.to_string().into_boxed_str(),
                        },
                        clock.now(),
                    ));
                }
                BlockingAttemptOutcome::WorkerSpawnFailed { message } => {
                    let error = controller.record_inactive_infrastructure_failure(
                        RetryInfrastructureFailure::WorkerSpawn { message },
                        clock.now(),
                    );
                    return Err(error);
                }
                BlockingAttemptOutcome::WorkerChannelClosed => {
                    return Err(controller.record_active_infrastructure_failure(
                        RetryInfrastructureFailure::WorkerChannelClosed,
                        clock.now(),
                    ));
                }
                BlockingAttemptOutcome::WorkerStillRunning { trigger } => {
                    let error = controller.record_active_infrastructure_failure(
                        RetryInfrastructureFailure::WorkerStillRunning { trigger },
                        clock.now(),
                    );
                    return Err(error);
                }
                BlockingAttemptOutcome::Stopped { trigger } => match trigger {
                    WorkerStopTrigger::TimerFailure => {
                        unreachable!("timer failure has a structured outcome")
                    }
                    WorkerStopTrigger::Cancellation => {
                        return Err(controller.record_attempt_cancellation(clock));
                    }
                    WorkerStopTrigger::AttemptTimeout => {
                        self.finish_failed_attempt(
                            timer,
                            &mut controller,
                            clock,
                            AttemptFailure::TimedOut {
                                scope: RetryTimeoutScope::Attempt,
                            },
                        )?;
                    }
                    WorkerStopTrigger::FlowTimeout => {
                        self.finish_failed_attempt(
                            timer,
                            &mut controller,
                            clock,
                            AttemptFailure::TimedOut {
                                scope: RetryTimeoutScope::Flow,
                            },
                        )?;
                    }
                },
                BlockingAttemptOutcome::Completed(Err(failure)) => {
                    self.finish_failed_attempt(timer, &mut controller, clock, failure)?;
                }
            }
        }
    }

    /// Records one attempt failure and performs the selected blocking delay.
    ///
    /// # Parameters
    /// - `controller`: Mutable state of this execution.
    /// - `clock`: Clock used for elapsed-time accounting.
    /// - `failure`: Failure owned by the completed attempt.
    ///
    /// # Returns
    /// Unit when another admission may be attempted.
    ///
    /// # Errors
    /// Returns a terminal control failure, cancellation, or backoff timer
    /// error.
    #[allow(
        clippy::result_large_err,
        reason = "the internal helper propagates the lossless public terminal error"
    )]
    fn finish_failed_attempt(
        &self,
        timer: &dyn Timer,
        controller: &mut RetryFlowController<'_, E>,
        clock: &dyn MonotonicClock,
        failure: AttemptFailure<E>,
    ) -> Result<(), RetryError<E>> {
        let directive = controller.record_failure(failure, clock, self.cancellation_token.as_ref())?;
        match wait_for_backoff(
            timer,
            directive.deadline(),
            directive.is_immediate(),
            self.cancellation_token.as_ref(),
        ) {
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
        Ok(())
    }
}
