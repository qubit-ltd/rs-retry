// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Worker-thread execution facade for blocking operations.

use std::future::Future;
use std::sync::Arc;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::BlockingSleeper;
use qubit_clock::MonotonicClock;
use qubit_clock::StdTimer;
use qubit_clock::TimeError;
use qubit_clock::Timer;

use super::attempt_cancellation_token::AttemptCancellationToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_attempt_outcome::BlockingAttemptOutcome;
use super::blocking_value_operation::BlockingValueOperation;
use super::internal::RetryFlowController;
use super::retry::Retry;
use super::retry_cancellation_token::RetryCancellationToken;
use super::worker_attempt_executor::WorkerAttemptExecutor;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::RetryTimeoutScope;
use crate::WorkerStopTrigger;
use crate::random::ThreadRetryRandomSource;

/// Result of waiting for one blocking retry delay.
enum BlockingBackoffOutcome {
    /// The configured delay elapsed.
    Elapsed,
    /// Flow cancellation interrupted the delay.
    Cancelled,
    /// Registering or polling the delay timer failed.
    TimerFailed(TimeError),
}

/// Waker that forwards timer and cancellation notifications to one channel.
struct BlockingBackoffWake {
    /// Notification sender shared by both futures.
    sender: mpsc::Sender<()>,
}

impl Wake for BlockingBackoffWake {
    /// Wakes the blocking retry thread through its notification channel.
    fn wake(self: Arc<Self>) {
        let _ = self.sender.send(());
    }

    /// Wakes the blocking retry thread without consuming the shared waker.
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.sender.send(());
    }
}

/// Worker retry execution with cooperative cancellation.
pub struct WorkerRetry<'a, E> {
    retry: &'a Retry<E>,
    thread_name: Box<str>,
    stack_size: Option<usize>,
    attempt_timeout: Option<Duration>,
    flow_timeout: Option<Duration>,
    cancellation_grace: Duration,
    cancellation_token: Option<RetryCancellationToken>,
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
            cancellation_token: None,
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

    /// Sets the token used to cancel the complete worker retry flow.
    ///
    /// # Parameters
    /// - `token`: Shared flow cancellation token.
    ///
    /// # Returns
    /// A worker facade that observes the supplied token.
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
        F: Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static,
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
            let cancellation = self.cancellation_token.as_ref();
            let _ = controller.before_attempt(clock, cancellation)?;
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&worker_operation),
                &self.thread_name,
                self.stack_size,
                self.cancellation_grace,
                cancellation,
                || {
                    let plan =
                        controller.commit_attempt(clock, cancellation)?;
                    Ok(plan.timeout())
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
                BlockingAttemptOutcome::WorkerStillRunning { trigger } => {
                    let error = controller
                        .record_active_infrastructure_failure(
                            RetryInfrastructureFailure::WorkerStillRunning {
                                trigger,
                            },
                            clock.now(),
                        );
                    return Err(error);
                }
                BlockingAttemptOutcome::Stopped { trigger } => match trigger {
                    WorkerStopTrigger::Cancellation => {
                        return Err(
                            controller.record_attempt_cancellation(clock)
                        );
                    }
                    WorkerStopTrigger::AttemptTimeout => {
                        self.finish_failed_attempt(
                            &mut controller,
                            clock,
                            AttemptFailure::TimedOut {
                                scope: RetryTimeoutScope::Attempt,
                            },
                        )?;
                    }
                    WorkerStopTrigger::FlowTimeout => {
                        self.finish_failed_attempt(
                            &mut controller,
                            clock,
                            AttemptFailure::TimedOut {
                                scope: RetryTimeoutScope::Flow,
                            },
                        )?;
                    }
                },
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
        clock: &dyn MonotonicClock,
        failure: AttemptFailure<E>,
    ) -> Result<(), RetryError<E>> {
        let directive = controller.record_failure(
            failure,
            clock,
            self.cancellation_token.as_ref(),
        )?;
        match self.wait_for_backoff(directive.sleep_duration()) {
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

    /// Waits for one retry delay while observing flow cancellation.
    ///
    /// # Parameters
    /// - `delay`: Selected backoff duration.
    ///
    /// # Returns
    /// Whether the delay elapsed, cancellation won, or the timer failed.
    /// Cancellation is polled before the timer so it wins when both become
    /// ready before the same wake cycle.
    fn wait_for_backoff(&self, delay: Duration) -> BlockingBackoffOutcome {
        let Some(token) = self.cancellation_token.as_ref() else {
            return match self.sleeper.sleep_for(delay) {
                Ok(()) => BlockingBackoffOutcome::Elapsed,
                Err(error) => BlockingBackoffOutcome::TimerFailed(error),
            };
        };
        if token.is_cancelled() {
            return BlockingBackoffOutcome::Cancelled;
        }
        let mut timer_future = match self.sleeper.timer().after(delay) {
            Ok(future) => future,
            Err(_) if token.is_cancelled() => {
                return BlockingBackoffOutcome::Cancelled;
            }
            Err(error) => return BlockingBackoffOutcome::TimerFailed(error),
        };
        let mut cancellation = Box::pin(token.cancelled());
        let (sender, receiver) = mpsc::channel();
        let waker = Waker::from(Arc::new(BlockingBackoffWake { sender }));
        let mut context = Context::from_waker(&waker);
        loop {
            if cancellation.as_mut().poll(&mut context).is_ready() {
                return BlockingBackoffOutcome::Cancelled;
            }
            if let Poll::Ready(result) =
                timer_future.as_mut().poll(&mut context)
            {
                return match result {
                    Ok(()) => BlockingBackoffOutcome::Elapsed,
                    Err(error) => BlockingBackoffOutcome::TimerFailed(error),
                };
            }
            receiver
                .recv()
                .expect("backoff futures must retain their shared waker");
        }
    }
}
