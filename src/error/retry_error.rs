// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Lossless retry execution errors.

use std::error::Error;
use std::fmt;

use crate::AttemptFailure;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryFailure;
use crate::RetrySuccess;

/// Error returned when a retry flow terminates without a successful result.
///
/// The terminal classification and all of its associated data live in one
/// [`RetryFailure`] value. The context is the coherent snapshot captured at
/// the same terminal decision.
///
/// # Type Parameters
/// - `E`: Owned application error retained without Clone, Display, or Error
///   requirements.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryError;
/// use qubit_retry::RetryPolicy;
///
/// let retry = Retry::<&str>::builder(RetryPolicy::builder().max_attempts(1).build()?).build();
/// let error: RetryError<&str> = retry.sync().run(|| Err::<(), _>("offline")).unwrap_err();
/// assert_eq!(error.last_error(), Some(&"offline"));
/// let (failure, context, diagnostics) = error.map_error(String::from).into_parts();
/// assert_eq!(failure.last_error().map(String::as_str), Some("offline"));
/// assert_eq!(context.attempts(), 1);
/// assert!(diagnostics.is_empty());
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
#[derive(Debug)]
pub struct RetryError<E> {
    /// Terminal retry-flow failure.
    failure: RetryFailure<E>,
    /// Context snapshot captured when the flow stopped.
    context: RetryContext,
    /// Panics raised while notifying completion observers.
    completion_callback_failures: Vec<RetryCallbackFailure>,
}

/// Result alias returned by retry executor execution.
/// # Type Parameters
/// - `T`: Successful operation value.
/// - `E`: Application error retained by a terminal retry failure.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;

impl<E> RetryError<E> {
    /// Creates a lossless retry error for executor-internal use.
    ///
    /// # Parameters
    /// - `failure`: Complete terminal failure value.
    /// - `context`: Context captured at the terminal decision.
    ///
    /// # Returns
    /// An error with the supplied frozen terminal state and no completion
    /// diagnostics.
    #[inline(always)]
    pub(crate) fn new(failure: RetryFailure<E>, context: RetryContext) -> Self {
        Self {
            failure,
            context,
            completion_callback_failures: Vec::new(),
        }
    }

    /// Returns the complete terminal failure.
    ///
    /// # Returns
    /// Borrowed terminal classification and its retained attempt data.
    #[inline(always)]
    #[must_use = "inspect the terminal failure"]
    pub const fn failure(&self) -> &RetryFailure<E> {
        &self.failure
    }

    /// Returns the retry context captured at termination.
    ///
    /// # Returns
    /// Borrowed frozen snapshot, excluding completion observer execution.
    #[inline(always)]
    #[must_use = "inspect the terminal retry context"]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns the last attempt failure retained by the terminal failure.
    ///
    /// # Returns
    /// `Some(&AttemptFailure<E>)` when an attempt failed before termination,
    /// or `None` when the flow stopped without an attempt failure.
    #[inline(always)]
    #[must_use]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.failure.last_failure()
    }

    /// Returns the last application error retained by the terminal failure.
    ///
    /// # Returns
    /// `Some(&E)` when the last attempt returned an application error, or
    /// `None` when no application error is retained.
    #[inline(always)]
    #[must_use]
    pub fn last_error(&self) -> Option<&E> {
        self.failure.last_error()
    }

    /// Returns completion observer panics in registration order.
    ///
    /// An empty slice means no completion callback panicked. These diagnostics
    /// do not change the operation result or the frozen terminal context.
    ///
    /// # Returns
    /// Borrowed ordered diagnostics; an empty slice means no completion panic.
    #[must_use = "inspect completion observer diagnostics"]
    #[inline(always)]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    /// Maps the retained application error while preserving terminal data.
    ///
    /// The mapper is called exactly once when the terminal failure retains an
    /// [`AttemptFailure::Error`], and is not called when no application error
    /// is retained. The context and completion callback diagnostics are moved
    /// into the returned error unchanged. A mapper panic propagates to the
    /// caller.
    ///
    /// # Type Parameters
    /// - `U`: New application error type.
    /// - `F`: Consuming mapper; no extra Clone, Send, or static bound is
    ///   required.
    ///
    /// # Parameters
    /// - `map`: Function called only when a retained application error exists.
    ///
    /// # Returns
    /// The same failure classification with its application payload converted.
    ///
    /// # Panics
    /// Propagates any panic raised by the mapper.
    #[inline(always)]
    pub fn map_error<U, F: FnOnce(E) -> U>(self, map: F) -> RetryError<U> {
        RetryError {
            failure: self.failure.map_error(map),
            context: self.context,
            completion_callback_failures: self.completion_callback_failures,
        }
    }

    /// Consumes this result, preserving its terminal data and diagnostics.
    ///
    /// Returns the (failure, context, completion callback failures) triple.
    ///
    /// # Returns
    /// Owned terminal failure, frozen context, and ordered completion
    /// diagnostics.
    #[must_use = "consume the terminal result, context and completion diagnostics"]
    #[inline(always)]
    pub fn into_parts(self) -> (RetryFailure<E>, RetryContext, Vec<RetryCallbackFailure>) {
        (self.failure, self.context, self.completion_callback_failures)
    }

    /// Consumes the error and returns its complete terminal failure.
    ///
    /// Discards the context and completion callback diagnostics.
    ///
    /// # Returns
    /// The lossless terminal [`RetryFailure`] value.
    #[inline(always)]
    #[must_use = "handle the terminal failure"]
    pub fn into_failure_discarding_diagnostics(self) -> RetryFailure<E> {
        self.failure
    }

    /// Attaches completion diagnostics after the final result is frozen.
    ///
    /// # Parameters
    /// - `failures`: Ordered completion diagnostics, replacing the initial
    ///   empty vector.
    #[inline(always)]
    pub(crate) fn set_completion_callback_failures(&mut self, failures: Vec<RetryCallbackFailure>) {
        self.completion_callback_failures = failures;
    }
}

impl<E: fmt::Display> fmt::Display for RetryError<E> {
    ///
    /// Formats the terminal reason and committed attempt count.
    ///
    /// # Parameters
    /// - `formatter`: Destination supplied by the formatting machinery.
    ///
    /// # Returns
    /// The result of writing this diagnostic representation.
    ///
    /// # Errors
    /// Returns a formatting error if the destination rejects a write.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} after {} attempt(s)",
            self.failure,
            self.context.attempts(),
        )
    }
}

impl<E> Error for RetryError<E>
where
    E: Error + 'static,
{
    /// Returns the last application error as the standard error source.
    ///
    /// # Returns
    /// `Some(source)` for a retained application error, or `None` for a
    /// terminal without a business error. Callback panic payloads do not
    /// fabricate sources.
    #[inline(always)]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error().map(|error| error as &(dyn Error + 'static))
    }
}
