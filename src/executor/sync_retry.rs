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

use super::internal::RetryFlowController;
use super::retry::Retry;
use crate::AttemptFailure;
use crate::RetryError;
use crate::RetryInfrastructureFailure;
use crate::RetryRandomSource;
use crate::RetrySuccess;
use crate::random::ThreadRetryRandomSource;

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
    #[allow(
        clippy::result_large_err,
        reason = "the public error intentionally retains lossless terminal context"
    )]
    pub fn run<T, F>(
        &self,
        mut operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let clock = self.sleeper.timer().clock();
        let mut controller = RetryFlowController::new(
            clock.now(),
            self.retry,
            Arc::clone(&self.random_source),
            None,
            None,
        );

        loop {
            let _ = controller.before_attempt(clock, None)?;
            controller.commit_attempt(clock, None)?;
            let result = operation();

            match result {
                Ok(value) => {
                    let context = controller.finish_success(clock)?;
                    return Ok(RetrySuccess::new(value, context));
                }
                Err(error) => {
                    let directive = controller.record_failure(
                        AttemptFailure::Error(error),
                        clock,
                        None,
                    )?;
                    if let Err(timer_error) =
                        self.sleeper.sleep_for(directive.sleep_duration())
                    {
                        let error = controller
                            .record_inactive_infrastructure_failure(
                                RetryInfrastructureFailure::Timer {
                                    message: timer_error
                                        .to_string()
                                        .into_boxed_str(),
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
