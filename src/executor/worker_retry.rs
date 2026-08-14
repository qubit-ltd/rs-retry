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

use qubit_clock::BlockingSleeper;
use qubit_clock::StdTimer;
use qubit_clock::Timer;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use super::blocking_value_operation::BlockingValueOperation;
use super::internal::EffectiveTimeout;
use super::internal::RetryFlowController;
use super::retry::Retry;
use super::worker_attempt_executor::WorkerAttemptExecutor;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;
use crate::random::ThreadRetryRandomSource;

/// Worker retry execution with cooperative cancellation.
pub struct WorkerRetry<'a, E> {
    retry: &'a Retry<E>,
    thread_name: Box<str>,
    stack_size: Option<usize>,
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
            thread_name: "qubit-retry-worker".into(),
            stack_size: None,
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

    /// Sets the OS-visible name assigned to each worker thread.
    ///
    /// # Parameters
    /// - `name`: Name passed to [`std::thread::Builder`].
    ///
    /// # Returns
    /// A worker facade using the supplied thread name.
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
    pub fn worker_stack_size(mut self, stack_size: usize) -> Self {
        self.stack_size = Some(stack_size);
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
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
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
        let mut controller = RetryFlowController::new(
            clock.now(),
            self.retry,
            Arc::clone(&self.random_source),
            self.attempt_timeout,
            self.flow_timeout,
        );

        loop {
            let _ = controller.before_attempt(clock, None)?;
            let mut effective_timeout = None;
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&worker_operation),
                &self.thread_name,
                self.stack_size,
                self.cancellation_grace,
                || {
                    let plan = controller.commit_attempt(clock, None)?;
                    effective_timeout = plan.timeout();
                    Ok(effective_timeout.map(EffectiveTimeout::duration))
                },
            )?;

            match outcome {
                BlockingAttemptOutcome::Completed(Ok(())) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(
                        operation.take_value(),
                        context,
                    ));
                }
                BlockingAttemptOutcome::WorkerSpawnFailed { message } => {
                    let error = controller
                        .record_inactive_infrastructure_failure(
                            RetryInfrastructureFailure::WorkerSpawn { message },
                            clock.now(),
                        );
                    return Err(error);
                }
                BlockingAttemptOutcome::WorkerStillRunning => {
                    let trigger = match effective_timeout
                        .expect(
                            "a worker only stops after an effective timeout",
                        )
                        .scope()
                    {
                        RetryTimeoutScope::Attempt => {
                            WorkerStopTrigger::AttemptTimeout
                        }
                        RetryTimeoutScope::Flow => {
                            WorkerStopTrigger::FlowTimeout
                        }
                    };
                    let error = controller
                        .record_active_infrastructure_failure(
                            RetryInfrastructureFailure::WorkerStillRunning {
                                trigger,
                            },
                            clock.now(),
                        );
                    return Err(error);
                }
                BlockingAttemptOutcome::TimedOut => {
                    let scope = effective_timeout
                        .expect(
                            "a worker only times out with an effective timeout",
                        )
                        .scope();
                    self.finish_failed_attempt(
                        &mut controller,
                        clock,
                        AttemptFailure::TimedOut { scope },
                    )?;
                }
                BlockingAttemptOutcome::Completed(Err(failure)) => {
                    self.finish_failed_attempt(
                        &mut controller,
                        clock,
                        failure,
                    )?;
                }
            }
        }
    }

    /// Records one attempt failure and performs the selected blocking delay.
    #[allow(
        clippy::result_large_err,
        reason = "the internal helper propagates the lossless public terminal error"
    )]
    fn finish_failed_attempt(
        &self,
        controller: &mut RetryFlowController<'_, E>,
        clock: &dyn qubit_clock::MonotonicClock,
        failure: AttemptFailure<E>,
    ) -> Result<(), RetryError<E>> {
        let directive = controller.record_failure(failure, clock, None)?;
        if let Err(timer_error) =
            self.sleeper.sleep_for(directive.sleep_duration())
        {
            let error = controller.record_inactive_infrastructure_failure(
                RetryInfrastructureFailure::Timer {
                    message: timer_error.to_string().into_boxed_str(),
                },
                clock.now(),
            );
            return Err(error);
        }
        Ok(())
    }
}
