// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry execution errors.
//!
//! This module contains the error returned when a retry executor stops without
//! a successful result. The original application error type is preserved in the
//! generic parameter `E`.

use std::error::Error;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::AttemptFailure;
use crate::RetryContext;
use crate::RetryErrorKind;
use crate::RetryErrorReason;
use crate::RetryExecutionError;
use crate::RetrySuccess;
use crate::event::AttemptTimeoutSource;

/// Error returned when a retry flow terminates without a successful result.
///
/// The generic parameter `E` is the caller's application error type. It is
/// preserved in [`AttemptFailure::Error`] when the terminal failure came from
/// the user operation. Runtime failures such as timeout, panic, and executor
/// failures are preserved through [`RetryError::last_failure`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(serialize = "E: Serialize", deserialize = "E: DeserializeOwned"))]
pub struct RetryError<E> {
    /// Terminal reason selected by the retry flow.
    reason: RetryErrorReason,
    /// Last attempt failure, if any attempt ran before termination.
    last_failure: Option<AttemptFailure<E>>,
    /// Retry infrastructure failure, when a control-path operation failed.
    execution_error: Option<RetryExecutionError>,
    /// Context snapshot captured when the retry flow stopped.
    context: RetryContext,
}

/// Result alias returned by retry executor execution.
///
/// The success type `T` is chosen by each operation. The error type `E`
/// remains the caller's original application error and is wrapped by
/// [`RetryError`] only when retry execution terminates unsuccessfully.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;

impl<E> RetryError<E> {
    /// Creates a retry error.
    ///
    /// # Arguments
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
            execution_error: None,
        }
    }

    /// Creates a retry error while preserving a control-path failure.
    #[allow(dead_code)]
    pub(crate) fn new_with_execution_error(
        reason: RetryErrorReason,
        last_failure: Option<AttemptFailure<E>>,
        execution_error: RetryExecutionError,
        context: RetryContext,
    ) -> Self {
        Self {
            reason,
            last_failure,
            execution_error: Some(execution_error),
            context,
        }
    }

    /// Returns the terminal retry error reason.
    ///
    /// # Returns
    /// The reason the retry flow stopped.
    #[inline(always)]
    pub fn reason(&self) -> RetryErrorReason {
        self.reason
    }

    /// Returns the stable terminal error category.
    pub fn kind(&self) -> RetryErrorKind {
        self.reason.kind()
    }

    /// Returns a control-path execution failure, if one exists.
    pub fn execution_error(&self) -> Option<&RetryExecutionError> {
        self.execution_error.as_ref()
    }

    /// Returns the retry context captured at termination.
    ///
    /// # Returns
    /// A context snapshot with attempt counts and timing metadata.
    #[inline(always)]
    pub fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns the timeout source that produced the final attempt timeout, if
    /// any.
    ///
    /// # Returns
    /// The timeout source when present, or `None` when no attempt timeout was
    /// selected for the terminal context.
    #[inline(always)]
    pub fn attempt_timeout_source(&self) -> Option<AttemptTimeoutSource> {
        self.context.attempt_timeout_source()
    }

    /// Returns the number of worker threads not observed to exit after
    /// cancellation.
    ///
    /// # Returns
    /// Count of timed-out worker attempts that did not finish within the worker
    /// cancellation grace period.
    #[inline(always)]
    pub fn unreaped_worker_count(&self) -> u32 {
        self.context.unreaped_worker_count()
    }

    /// Returns the number of attempts admitted into execution.
    ///
    /// `before_attempt` receives the upcoming one-based attempt number before
    /// it is committed. If a pre-attempt listener exhausts a budget, this count
    /// does not include that unexecuted attempt.
    /// In particular, the first `before_attempt` callback may see `1` while the
    /// operation runs zero times and this method returns `0`.
    ///
    /// # Returns
    /// The committed operation-attempt count at termination.
    #[inline(always)]
    pub fn attempts(&self) -> u32 {
        self.context.attempt()
    }

    /// Returns the last failure, if one exists.
    ///
    /// # Returns
    /// `Some(&AttemptFailure<E>)` when at least one attempt failure was
    /// observed; `None` when the retry flow stopped before any attempt ran.
    #[inline(always)]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.last_failure.as_ref()
    }

    /// Returns the last application error, if one exists.
    ///
    /// # Returns
    /// `Some(&E)` when the terminal failure wraps an application error;
    /// `None` for timeout, panic, executor failures, or elapsed-budget failures
    /// with no attempt.
    #[inline(always)]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure().and_then(AttemptFailure::as_error)
    }

    /// Consumes the retry error and returns the last application error when
    /// the final failure wraps one.
    ///
    /// # Returns
    /// `Some(E)` when the terminal failure owns an application error; `None`
    /// when the terminal failure was a timeout, panic, executor failure, or
    /// when no attempt ran.
    #[inline(always)]
    pub fn into_last_error(self) -> Option<E> {
        self.last_failure.and_then(AttemptFailure::into_error)
    }

    /// Consumes the retry error and returns all terminal parts.
    ///
    /// # Returns
    /// A tuple `(reason, last_failure, context)` preserving all terminal data.
    #[inline(always)]
    pub fn into_parts(self) -> (RetryErrorReason, Option<AttemptFailure<E>>, RetryContext) {
        (self.reason, self.last_failure, self.context)
    }

    /// Consumes the error and returns all terminal data including execution
    /// infrastructure diagnostics.
    pub fn into_parts_with_execution_error(
        self,
    ) -> (
        RetryErrorReason,
        Option<AttemptFailure<E>>,
        Option<RetryExecutionError>,
        RetryContext,
    ) {
        (
            self.reason,
            self.last_failure,
            self.execution_error,
            self.context,
        )
    }
}

impl<E> fmt::Display for RetryError<E>
where
    E: fmt::Display,
{
    /// Formats the retry error for diagnostics.
    ///
    /// # Arguments
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
            RetryErrorReason::SleeperFailed => {
                format!("retry sleeper failed after {attempts} attempt(s)")
            }
            RetryErrorReason::WorkerStillRunning => {
                format!(
                    "retry worker still running after timeout cancellation grace, unreaped {}",
                    self.context.unreaped_worker_count()
                )
            }
            RetryErrorReason::AttemptTimedOut => {
                format!("retry attempt timed out after {attempts} attempt(s)")
            }
            RetryErrorReason::FlowTimedOut => {
                format!("retry flow timed out after {attempts} attempt(s)")
            }
            RetryErrorReason::TimerFailed => {
                format!("retry timer failed after {attempts} attempt(s)")
            }
            RetryErrorReason::AttemptsExhausted => format!(
                "retry attempts exhausted after {attempts} attempt(s), max {}",
                self.context.max_attempts()
            ),
            RetryErrorReason::OperationBudgetExhausted => {
                format!("retry operation budget exhausted after {attempts} attempt(s)")
            }
            RetryErrorReason::TotalBudgetExhausted => {
                format!("retry total budget exhausted after {attempts} attempt(s)")
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
    /// # Returns
    /// `Some(&dyn Error)` when the terminal failure wraps an application error,
    /// captured panic, or executor failure; otherwise `None`.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.last_failure() {
            Some(AttemptFailure::Error(error)) => Some(error as &(dyn Error + 'static)),
            Some(AttemptFailure::Panic(panic)) => Some(panic as &(dyn Error + 'static)),
            Some(AttemptFailure::Executor(error)) => Some(error as &(dyn Error + 'static)),
            Some(AttemptFailure::Infrastructure(error)) => Some(error as &(dyn Error + 'static)),
            Some(AttemptFailure::Timeout) | None => self
                .execution_error
                .as_ref()
                .map(|error| error as &(dyn Error + 'static)),
        }
    }
}
