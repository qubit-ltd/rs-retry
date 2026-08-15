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

/// Observes retry lifecycle events without changing retry decisions.
pub trait RetryObserver<E>: Send + Sync + 'static {
    /// Observes the context before an attempt is admitted.
    fn on_attempt_started(&self, _context: &RetryContext) {}

    /// Observes one committed attempt failure.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<E>,
        _context: &RetryContext,
    ) {
    }

    /// Observes one selected retry delay.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
    }
}

impl<E, F> RetryObserver<E> for F
where
    F: Fn(&AttemptFailure<E>, &RetryContext) + Send + Sync + 'static,
{
    fn on_attempt_failed(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) {
        self(failure, context);
    }
}
