/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry execution errors.
//!
//! This module contains the error returned when a retry executor stops without a
//! successful result. The original application error type is preserved in the
//! generic parameter `E`.

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

use crate::{AttemptFailure, RetryContext, RetryErrorReason};

/// Error returned when a retry flow terminates without a successful result.
///
/// The generic parameter `E` is the caller's application error type. It is
/// preserved in [`AttemptFailure::Error`] when the terminal failure came from
/// the user operation. Runtime failures such as timeout, panic, and executor
/// failures are preserved through [`RetryError::last_failure`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "E: serde::Serialize",
    deserialize = "E: serde::de::DeserializeOwned"
))]
pub struct RetryError<E> {
    /// Terminal reason selected by the retry flow.
    reason: RetryErrorReason,
    /// Last attempt failure, if any attempt ran before termination.
    last_failure: Option<AttemptFailure<E>>,
    /// Context snapshot captured when the retry flow stopped.
    context: RetryContext,
}

/// Result alias returned by retry executor execution.
///
/// The success type `T` is chosen by each operation. The error type `E`
/// remains the caller's original application error and is wrapped by
/// [`RetryError`] only when retry execution terminates unsuccessfully.
pub type RetryResult<T, E> = Result<T, RetryError<E>>;

impl<E> RetryError<E> {
    /// Creates a retry error.
    ///
    /// # Parameters
    /// - `reason`: Terminal reason.
    /// - `last_failure`: Last observed attempt failure, if any.
    /// - `context`: Retry context captured at termination.
    ///
    /// # Returns
    /// A retry error preserving the terminal reason and context.
    #[inline]
    pub(crate) fn new(
        reason: RetryErrorReason,
        last_failure: Option<AttemptFailure<E>>,
        context: RetryContext,
    ) -> Self {
        Self {
            reason,
            last_failure,
            context,
        }
    }

    /// Returns the terminal retry error reason.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The reason the retry flow stopped.
    #[inline]
    pub fn reason(&self) -> RetryErrorReason {
        self.reason
    }

    /// Returns the retry context captured at termination.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// A context snapshot with attempt counts and timing metadata.
    #[inline]
    pub fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns the timeout source that produced the final attempt timeout, if any.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The timeout source when present, or `None` when no attempt timeout was
    /// selected for the terminal context.
    #[inline]
    pub fn attempt_timeout_source(&self) -> Option<crate::event::AttemptTimeoutSource> {
        self.context.attempt_timeout_source()
    }

    /// Returns the number of worker threads not observed to exit after cancellation.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// Count of timed-out worker attempts that did not finish within the worker
    /// cancellation grace period.
    #[inline]
    pub fn unreaped_worker_count(&self) -> u32 {
        self.context.unreaped_worker_count()
    }

    /// Returns the number of attempts that were executed.
    ///
    /// # Returns
    /// The number of operation attempts observed before termination.
    #[inline]
    pub fn attempts(&self) -> u32 {
        self.context.attempt()
    }

    /// Returns the last failure, if one exists.
    ///
    /// # Returns
    /// `Some(&AttemptFailure<E>)` when at least one attempt failure was observed;
    /// `None` when the retry flow stopped before any attempt ran.
    #[inline]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.last_failure.as_ref()
    }

    /// Returns the last application error, if one exists.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&E)` when the terminal failure wraps an application error;
    /// `None` for timeout, panic, executor failures, or elapsed-budget failures
    /// with no attempt.
    #[inline]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure().and_then(AttemptFailure::as_error)
    }

    /// Consumes the retry error and returns the last application error when
    /// the final failure wraps one.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(E)` when the terminal failure owns an application error; `None`
    /// when the terminal failure was a timeout, panic, executor failure, or
    /// when no attempt ran.
    #[inline]
    pub fn into_last_error(self) -> Option<E> {
        self.last_failure.and_then(AttemptFailure::into_error)
    }

    /// Consumes the retry error and returns all terminal parts.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// A tuple `(reason, last_failure, context)` preserving all terminal data.
    #[inline]
    pub fn into_parts(self) -> (RetryErrorReason, Option<AttemptFailure<E>>, RetryContext) {
        (self.reason, self.last_failure, self.context)
    }
}

impl<E> fmt::Display for RetryError<E>
where
    E: fmt::Display,
{
    /// Formats the retry error for diagnostics.
    ///
    /// # Parameters
    /// - `f`: Formatter provided by the standard formatting machinery.
    ///
    /// # Returns
    /// `fmt::Result` from the formatter.
    ///
    /// # Errors
    /// Returns a formatting error if the underlying formatter fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let attempts = self.attempts();
        let message = match self.reason {
            RetryErrorReason::Aborted => format!("retry aborted after {attempts} attempt(s)"),
            RetryErrorReason::AttemptsExceeded => format!(
                "retry attempts exceeded after {attempts} attempt(s), max {}",
                self.context.max_attempts()
            ),
            RetryErrorReason::MaxOperationElapsedExceeded => {
                format!("retry max operation elapsed exceeded after {attempts} attempt(s)")
            }
            RetryErrorReason::MaxTotalElapsedExceeded => {
                format!("retry max total elapsed exceeded after {attempts} attempt(s)")
            }
            RetryErrorReason::UnsupportedOperation => {
                "run() does not support attempt timeout; use run_async() or run_in_worker()"
                    .to_string()
            }
            RetryErrorReason::WorkerStillRunning => {
                format!(
                    "retry worker still running after timeout cancellation grace, unreaped {}",
                    self.context.unreaped_worker_count()
                )
            }
        };
        f.write_str(&message)?;
        if let Some(failure) = &self.last_failure {
            write!(f, "; last failure: {failure}")?;
        }
        Ok(())
    }
}

impl<E> Error for RetryError<E>
where
    E: Error + 'static,
{
    /// Returns the source terminal failure when one is available.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&dyn Error)` when the terminal failure wraps an application error,
    /// captured panic, or executor failure; otherwise `None`.
    ///
    /// # Errors
    /// This method does not return errors.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.last_failure() {
            Some(AttemptFailure::Error(error)) => Some(error as &(dyn Error + 'static)),
            Some(AttemptFailure::Panic(panic)) => Some(panic as &(dyn Error + 'static)),
            Some(AttemptFailure::Executor(error)) => Some(error as &(dyn Error + 'static)),
            Some(AttemptFailure::Timeout) | None => None,
        }
    }
}
