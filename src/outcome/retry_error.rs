// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal errors produced by retry executors.

use std::error::Error;
use std::fmt;

use crate::AttemptFailure;
use crate::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorMetadata;
use crate::RetryErrorReason;

/// Error returned when a retry flow terminates without a successful result.
///
/// The terminal `reason` and `context` are frozen when the flow ends. The
/// retained `last_failure` can come from an earlier attempt if a later
/// cancellation or infrastructure failure ends the flow. The
/// `completion_callback_failures` are diagnostics recorded after that
/// terminal snapshot.
///
/// # Examples
/// ```
/// use qubit_retry::{Retry, RetryConfig, RetryErrorReason, RetryFallback};
///
/// let config = RetryConfig::<&'static str>::builder()
///     .max_attempts(1)
///     .fallback(RetryFallback::Retry)
///     .build()
///     .unwrap();
/// let error = Retry::new(&config)
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

    /// Returns the most recently retained attempt failure, when one exists.
    ///
    /// This failure may belong to an earlier attempt. Cancellation or an
    /// infrastructure failure can terminate a later admitted attempt without
    /// replacing it. Inspect `reason()` and `context()` together for
    /// attribution.
    #[must_use = "inspect the last attempt failure"]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.last_failure.as_ref()
    }

    /// Returns the application error in the most recently retained failure.
    ///
    /// The error may belong to an earlier attempt, rather than the last
    /// admitted operation. Inspect `reason()` and `context()` together for
    /// attribution.
    ///
    /// # Returns
    /// `Some` when the retained failure contains an application error;
    /// otherwise `None`, including when no attempt failure has been
    /// retained.
    #[must_use = "inspect the application error"]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure.as_ref().and_then(AttemptFailure::as_error)
    }

    /// Returns completion callback failures captured after the terminal result
    /// was frozen.
    ///
    /// # Returns
    /// A borrowed slice containing each callback panic captured after the
    /// terminal reason and context were frozen; it is empty when none failed.
    #[must_use = "inspect completion callback diagnostics"]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    /// Maps a retained application error while preserving terminal metadata
    /// and failure classification.
    ///
    /// This consumes the error. The mapper is called exactly once if the
    /// retained last failure contains an application error, and zero times for
    /// timeout, panic, or no retained failure. The mapped error replaces only
    /// that application error; the terminal reason, retained attempt metadata,
    /// context, and completion callback diagnostics are preserved.
    ///
    /// # Type Parameters
    /// - `U`: Type of the mapped application error.
    /// - `F`: One-shot mapping function.
    ///
    /// # Parameters
    /// - `map`: Function applied to the retained application error, if any.
    ///
    /// # Returns
    /// A terminal error with the same reason, context, and diagnostics, and
    /// with its retained application error mapped when present.
    ///
    /// # Panics
    /// A panic from `map` propagates to the caller; it is not converted into a
    /// callback failure.
    pub fn map_error<U, F: FnOnce(E) -> U>(self, map: F) -> RetryError<U> {
        RetryError {
            reason: self.reason,
            last_failure: self.last_failure.map(|failure| failure.map_error(map)),
            context: self.context,
            completion_callback_failures: self.completion_callback_failures,
        }
    }

    /// Consumes the error into its terminal reason, retained failure, frozen
    /// context, and completion callback diagnostics.
    ///
    /// The returned tuple contains, in order: the terminal reason; the
    /// optional retained attempt failure; the frozen retry context; and a
    /// boxed slice of completion callback failures recorded after the
    /// terminal state was frozen.
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
    ///
    /// The returned metadata retains the terminal reason, context, completion
    /// callback diagnostics, and classification of the last retained failure.
    /// The application error is moved out when the last failure contains one;
    /// otherwise the returned error is `None`.
    ///
    /// # Type Parameters
    /// `E` is the application error type, moved into the optional return value
    /// when the retained last failure contains an application error.
    ///
    /// # Returns
    /// The terminal metadata and the moved application error, if the retained
    /// last failure was an application error.
    #[must_use = "retain the terminal metadata and optional application error"]
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

impl<E> fmt::Display for RetryError<E> {
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
    /// Exposes the application error in the most recently retained failure.
    ///
    /// The error may belong to an earlier attempt. Cancellation or an
    /// infrastructure failure can terminate a later admitted attempt without
    /// replacing it. Inspect `reason()` and `context()` together for
    /// attribution.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error().map(|error| error as &(dyn Error + 'static))
    }
}
