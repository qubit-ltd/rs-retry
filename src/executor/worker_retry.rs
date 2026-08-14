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
use qubit_clock::TimeError;
use qubit_clock::Timer;

use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt::BlockingAttempt;
use super::blocking_value_operation::BlockingValueOperation;
use super::internal::EffectiveTimeout;
use super::internal::RetryFlowState;
use super::retry::Retry;
use super::worker_attempt_executor::WorkerAttemptExecutor;
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
    #[allow(clippy::result_large_err)]
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
                let context = flow.context(snapshot, snapshot.attempts());
                return Err(self.finish(reason, last_failure, context));
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
            let effective_timeout =
                flow.effective_timeout(self.attempt_timeout);
            let outcome = WorkerAttemptExecutor::run(
                Arc::clone(&worker_operation),
                &self.thread_name,
                self.stack_size,
                effective_timeout.map(EffectiveTimeout::duration),
                self.cancellation_grace,
            );
            let snapshot = flow.finish_attempt(attempt);
            let mut attempt_context = flow
                .context(snapshot, snapshot.attempts())
                .with_attempt_timeout(
                    effective_timeout.map(EffectiveTimeout::duration),
                )
                .with_unreaped_worker_count(outcome.unreaped_worker_count);
            let result = match outcome.result {
                Ok(()) => Ok(()),
                Err(AttemptFailure::Timeout { .. }) => {
                    let kind = effective_timeout
                        .map(EffectiveTimeout::kind)
                        .unwrap_or(AttemptTimeoutKind::Attempt);
                    Err(AttemptFailure::Timeout { kind })
                }
                Err(failure) => Err(failure),
            };
            match result {
                Ok(()) => {
                    self.retry.observers().finished(
                        RetryOutcomeKind::Succeeded,
                        &attempt_context,
                    );
                    return Ok(RetrySuccess::new(
                        operation.take_value(),
                        attempt_context,
                    ));
                }
                Err(failure) => {
                    self.retry
                        .observers()
                        .attempt_failed(&failure, &attempt_context);
                    if attempt_context.unreaped_worker_count() > 0 {
                        return Err(self.finish(
                            RetryErrorReason::WorkerStillRunning,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if matches!(failure, AttemptFailure::Infrastructure(_)) {
                        let detail = failure
                            .execution_error()
                            .map(|error| error.message().to_owned())
                            .unwrap_or_else(|| {
                                "worker execution failed".to_owned()
                            });
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
                    let hint = decision.retry_after_hint();
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
                    attempt_context = attempt_context
                        .with_next_delay(step.effective_delay())
                        .with_retry_after_hint(hint);
                    self.retry
                        .observers()
                        .retry_scheduled(&step, &attempt_context);
                    if let Some(reason) =
                        flow.retry_reason(step.effective_delay())
                    {
                        return Err(self.finish(
                            reason,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if let Some(remaining) =
                        flow.flow_sleep_cap(step.effective_delay())
                    {
                        if let Err(error) = self.sleeper.sleep_for(remaining) {
                            return Err(self.finish_with_timer_error(
                                Some(failure),
                                attempt_context,
                                error,
                            ));
                        }
                        return Err(self.finish(
                            RetryErrorReason::FlowTimedOut,
                            Some(failure),
                            attempt_context,
                        ));
                    }
                    if let Err(error) =
                        self.sleeper.sleep_for(step.effective_delay())
                    {
                        return Err(self.finish_with_timer_error(
                            Some(failure),
                            attempt_context,
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
        let error = RetryError::new_with_execution_error(
            RetryErrorReason::TimerFailed,
            failure,
            crate::RetryExecutionError::timer(&error.to_string()),
            context,
        );
        self.retry
            .observers()
            .finished(RetryOutcomeKind::Failed, error.context());
        error
    }

    fn finish_with_execution_error(
        &self,
        failure: Option<AttemptFailure<E>>,
        context: RetryContext,
        message: &str,
    ) -> RetryError<E> {
        let error = RetryError::new_with_execution_error(
            RetryErrorReason::WorkerFailed,
            failure,
            crate::RetryExecutionError::worker(message),
            context,
        );
        self.retry
            .observers()
            .finished(RetryOutcomeKind::Failed, error.context());
        error
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

fn terminal_reason<E>(failure: &AttemptFailure<E>) -> RetryErrorReason {
    match failure {
        AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Attempt,
        } => RetryErrorReason::AttemptTimedOut,
        AttemptFailure::Error(_)
        | AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Flow,
        }
        | AttemptFailure::Panic => RetryErrorReason::Aborted,
        AttemptFailure::Infrastructure(_) => RetryErrorReason::WorkerFailed,
    }
}
