// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Same-thread execution facade for the pure retry policy API.

use std::sync::Arc;

use qubit_clock::BlockingSleeper;
use qubit_clock::StdTimer;
use qubit_clock::Timer;

use super::internal::RetryFlowState;
use super::retry::Retry;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryErrorReason;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::observer::RetryOutcomeKind;
use crate::random::ThreadRetryRandomSource;
use crate::rule::RetryDecision;

/// Same-thread retry execution. It intentionally exposes no timeout method.
pub struct SyncRetry<'a, E> {
    retry: &'a Retry<E>,
    sleeper: BlockingSleeper,
    random_source: Arc<dyn RetryRandomSource>,
}

impl<'a, E: 'static> SyncRetry<'a, E> {
    /// Creates a synchronous facade from one retry policy.
    pub(crate) fn new(retry: &'a Retry<E>) -> Self {
        Self {
            retry,
            sleeper: BlockingSleeper::new(Arc::new(StdTimer::new())),
            random_source: Arc::new(ThreadRetryRandomSource),
        }
    }

    /// Replaces the blocking timer used by this execution.
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.sleeper = BlockingSleeper::new(timer);
        self
    }

    /// Replaces the random source used by this execution.
    pub fn random_source(mut self, random: Arc<dyn RetryRandomSource>) -> Self {
        self.random_source = random;
        self
    }

    /// Runs a same-thread operation until success or a terminal retry error.
    #[allow(clippy::result_large_err)]
    pub fn run<T, F>(
        &self,
        mut operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let clock = self.sleeper.timer().clock();
        let mut flow = RetryFlowState::new(
            clock,
            self.retry.policy(),
            Arc::clone(&self.random_source),
            None,
        )
        .expect("validated retry limits must fit the monotonic clock");
        let mut last_failure = None;

        loop {
            if let Some(reason) = flow.continuation_reason() {
                let error = RetryError::new(
                    reason,
                    last_failure,
                    flow.current_context(),
                );
                self.retry
                    .observers()
                    .finished(RetryOutcomeKind::Failed, error.context());
                return Err(error);
            }

            let upcoming = flow.upcoming_context();
            self.retry.observers().attempt_started(&upcoming);

            let attempt = match flow.begin_attempt() {
                Ok(attempt) => attempt,
                Err(reason) => {
                    let error = RetryError::new(
                        reason,
                        last_failure,
                        flow.current_context(),
                    );
                    self.retry
                        .observers()
                        .finished(RetryOutcomeKind::Failed, error.context());
                    return Err(error);
                }
            };
            let result = operation();
            let snapshot = flow.finish_attempt(attempt);
            let attempt_context = flow.context(snapshot, snapshot.attempts());

            match result {
                Ok(value) => {
                    self.retry.observers().finished(
                        RetryOutcomeKind::Succeeded,
                        &attempt_context,
                    );
                    return Ok(RetrySuccess::new(value, attempt_context));
                }
                Err(error) => {
                    let failure = AttemptFailure::Error(error);
                    self.retry
                        .observers()
                        .attempt_failed(&failure, &attempt_context);
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
                    let hint = decision.retry_after_hint();
                    let decision = default_decision(decision);
                    if matches!(decision, RetryDecision::Abort) {
                        let error = RetryError::new(
                            RetryErrorReason::Aborted,
                            Some(failure),
                            attempt_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if let Some(reason) = flow.continuation_reason() {
                        let error = RetryError::new(
                            reason,
                            Some(failure),
                            attempt_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }

                    let step = flow.next_backoff(decision);
                    let scheduled_context = flow
                        .current_context()
                        .with_next_delay(step.effective_delay())
                        .with_retry_after_hint(hint);
                    self.retry
                        .observers()
                        .retry_scheduled(&step, &scheduled_context);
                    if let Some(reason) =
                        flow.retry_reason(step.effective_delay())
                    {
                        let error = RetryError::new(
                            reason,
                            Some(failure),
                            scheduled_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    if let Err(timer_error) =
                        self.sleeper.sleep_for(step.effective_delay())
                    {
                        let error = RetryError::new_with_execution_error(
                            RetryErrorReason::TimerFailed,
                            Some(failure),
                            crate::RetryExecutionError::timer(
                                &timer_error.to_string(),
                            ),
                            scheduled_context,
                        );
                        self.retry.observers().finished(
                            RetryOutcomeKind::Failed,
                            error.context(),
                        );
                        return Err(error);
                    }
                    last_failure = Some(failure);
                }
            }
        }
    }
}

fn default_decision(decision: RetryDecision) -> RetryDecision {
    if !matches!(decision, RetryDecision::UseDefault) {
        return decision;
    }
    RetryDecision::Retry
}
