// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry observer trait.

use crate::AttemptFailure;
use crate::BackoffStep;
use crate::RetryContext;
use crate::RetryFailure;

/// Observes retry lifecycle events without changing retry decisions.
pub trait RetryObserver<E>: Send + Sync + 'static {
    /// Observes a successful flow with its frozen terminal context.
    ///
    /// Called once in registration order before `run` returns `Ok`. This
    /// synchronous callback must remain short and nonblocking. Its elapsed
    /// time is excluded from `context`; a panic becomes an attached completion
    /// diagnostic and does not replace the successful result.
    /// Operation panics, dropped async futures and process aborts do not
    /// guarantee completion notification.
    fn on_success(&self, _context: &RetryContext) {}

    /// Observes a terminal failure with its frozen context.
    ///
    /// Called once in registration order before `run` returns `Err`, including
    /// failures before any attempt is admitted. A panic is attached as a
    /// completion diagnostic while later observers still run; it does not
    /// change `failure` or trigger retries or another terminal notification.
    /// This synchronous callback must remain short and nonblocking. Its time
    /// is excluded from `context` and cannot be interrupted by retry timeouts.
    /// Operation panics, dropped async futures and process aborts do not
    /// guarantee completion notification.
    fn on_terminal_failure(
        &self,
        _failure: &RetryFailure<E>,
        _context: &RetryContext,
    ) {
    }

    /// Observes the context before an attempt is admitted.
    fn on_attempt_started(&self, _context: &RetryContext) {}

    /// Observes one committed attempt failure.
    fn on_attempt_failed(&self, _failure: &AttemptFailure<E>, _context: &RetryContext) {}

    /// Observes a delay that currently fits the continuation budgets.
    ///
    /// This callback is not emitted when the flow is already exhausted. The
    /// controller checks budgets and cancellation again after callbacks and
    /// after sleep, so scheduling does not promise another admitted attempt.
    /// Use terminal context attempts to count actual operations.
    fn on_retry_scheduled(&self, _backoff: &BackoffStep, _context: &RetryContext) {}
}

impl<E, F> RetryObserver<E> for F
where
    F: Fn(&AttemptFailure<E>, &RetryContext) + Send + Sync + 'static,
{
    fn on_attempt_failed(&self, failure: &AttemptFailure<E>, context: &RetryContext) {
        self(failure, context);
    }
}
