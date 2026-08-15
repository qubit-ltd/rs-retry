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
use crate::RetryContext;
use crate::RetryFailure;
use crate::RetrySuccess;

/// Error returned when a retry flow terminates without a successful result.
///
/// The terminal classification and all of its associated data live in one
/// [`RetryFailure`] value. The context is the coherent snapshot captured at
/// the same terminal decision.
#[must_use]
#[derive(Debug)]
pub struct RetryError<E> {
    /// Terminal retry-flow failure.
    failure: RetryFailure<E>,
    /// Context snapshot captured when the flow stopped.
    context: RetryContext,
}

/// Result alias returned by retry executor execution.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;

impl<E> RetryError<E> {
    /// Creates a lossless retry error for executor-internal use.
    ///
    /// # Arguments
    /// - `failure`: Complete terminal failure value.
    /// - `context`: Context captured at the terminal decision.
    #[inline(always)]
    pub(crate) fn new(failure: RetryFailure<E>, context: RetryContext) -> Self {
        Self { failure, context }
    }

    /// Returns the complete terminal failure.
    #[inline(always)]
    #[must_use]
    pub const fn failure(&self) -> &RetryFailure<E> {
        &self.failure
    }

    /// Returns the retry context captured at termination.
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

    /// Consumes the error and returns its complete terminal failure.
    ///
    /// # Returns
    /// The lossless terminal [`RetryFailure`] value.
    #[inline(always)]
    #[must_use]
    pub fn into_failure(self) -> RetryFailure<E> {
        self.failure
    }

    /// Consumes the error and returns its complete terminal data.
    ///
    /// # Returns
    /// The lossless `(failure, context)` pair.
    #[inline(always)]
    pub fn into_parts(self) -> (RetryFailure<E>, RetryContext) {
        (self.failure, self.context)
    }
}

impl<E: fmt::Display> fmt::Display for RetryError<E> {
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
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error()
            .map(|error| error as &(dyn Error + 'static))
    }
}
