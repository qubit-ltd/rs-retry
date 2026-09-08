// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal retry errors and their result alias.

use std::error::Error;
use std::fmt;

use crate::AttemptFailure;
use crate::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorMetadata;
use crate::RetryErrorReason;
use crate::RetrySuccess;

/// Error returned when a retry flow terminates without a successful result.
///
/// # Examples
/// ```
/// use qubit_retry::{Retry, RetryErrorReason, RetryFallback, RetryPolicy};
///
/// let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
/// let retry = Retry::<&'static str>::builder(policy)
///     .fallback(RetryFallback::Retry)
///     .build();
/// let error = retry
///     .sync()
///     .run(|| Err::<(), _>("offline"))
///     .unwrap_err();
/// assert!(matches!(error.reason(), RetryErrorReason::Exhausted { .. }));
/// ```
#[must_use]
#[derive(Debug)]
pub struct RetryError<E> {
    reason: RetryErrorReason,
    last_failure: Option<AttemptFailure<E>>,
    context: RetryContext,
    completion_callback_failures: Box<[RetryCallbackFailure]>,
}

/// Result alias returned by retry executor execution.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;

impl<E> RetryError<E> {
    /// Creates a terminal error before completion callbacks are notified.
    pub(crate) fn new(
        reason: RetryErrorReason,
        last_failure: Option<AttemptFailure<E>>,
        context: RetryContext,
    ) -> Self {
        Self {
            reason,
            last_failure,
            context,
            completion_callback_failures: Box::new([]),
        }
    }

    /// Returns the reason that terminated the retry flow.
    #[must_use = "inspect the terminal reason"]
    pub const fn reason(&self) -> &RetryErrorReason {
        &self.reason
    }

    /// Returns the frozen context at which the retry flow terminated.
    #[must_use = "inspect the terminal context"]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns the most recent attempt failure, when one exists.
    #[must_use = "inspect the last attempt failure"]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.last_failure.as_ref()
    }

    /// Returns the application error from the most recent attempt, when one
    /// exists.
    #[must_use = "inspect the application error"]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure.as_ref().and_then(AttemptFailure::as_error)
    }

    /// Returns completion callback failures captured after the terminal result
    /// was frozen.
    #[must_use = "inspect completion callback diagnostics"]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    /// Maps an application error while preserving terminal metadata and failure
    /// classification.
    ///
    /// The mapper runs only when the last failure contains an application
    /// error.
    pub fn map_error<U, F: FnOnce(E) -> U>(self, map: F) -> RetryError<U> {
        RetryError {
            reason: self.reason,
            last_failure: self.last_failure.map(|failure| failure.map_error(map)),
            context: self.context,
            completion_callback_failures: self.completion_callback_failures,
        }
    }

    /// Consumes the error into its reason, last failure, context, and callback
    /// diagnostics.
    #[must_use = "inspect the decomposed terminal error"]
    pub fn into_parts(
        self,
    ) -> (
        RetryErrorReason,
        Option<AttemptFailure<E>>,
        RetryContext,
        Box<[RetryCallbackFailure]>,
    ) {
        (
            self.reason,
            self.last_failure,
            self.context,
            self.completion_callback_failures,
        )
    }

    /// Consumes the error into non-generic metadata and an optional application
    /// error.
    #[must_use]
    pub fn into_metadata_and_error(self) -> (RetryErrorMetadata, Option<E>) {
        let (reason, last_failure, context, completion_callback_failures) = self.into_parts();
        let (last_attempt, application_error) = match last_failure {
            Some(AttemptFailure::Error(error)) => (Some(AttemptFailureMetadata::ApplicationError), Some(error)),
            Some(AttemptFailure::TimedOut { scope }) => (Some(AttemptFailureMetadata::TimedOut { scope }), None),
            Some(AttemptFailure::Panicked { panic }) => (Some(AttemptFailureMetadata::Panicked { panic }), None),
            None => (None, None),
        };
        (
            RetryErrorMetadata {
                reason,
                last_attempt,
                context,
                completion_callback_failures,
            },
            application_error,
        )
    }

    /// Stores completion callback diagnostics on the frozen terminal error.
    pub(crate) fn set_completion_callback_failures(&mut self, failures: Vec<RetryCallbackFailure>) {
        self.completion_callback_failures = failures.into_boxed_slice();
    }
}

impl<E: fmt::Display> fmt::Display for RetryError<E> {
    /// Formats the terminal reason and number of admitted attempts.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "retry {} after {} attempt(s)",
            self.reason,
            self.context.attempts()
        )
    }
}

impl<E: Error + 'static> Error for RetryError<E> {
    /// Exposes the last application error as the standard error source.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error().map(|error| error as &(dyn Error + 'static))
    }
}
