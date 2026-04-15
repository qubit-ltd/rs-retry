/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry execution errors.
//!
//! This module contains the error returned when a retry executor stops without a
//! successful result. The original application error type is preserved in the
//! generic parameter `E`.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use super::RetryAttemptFailure;

/// Error returned when a retry executor terminates without a successful result.
///
/// The generic parameter `E` is the caller's application error type. It is
/// preserved in the final [`RetryAttemptFailure`] whenever the terminal failure came
/// from the user operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryError<E> {
    /// The configured classifier marked the last failure as non-retryable.
    Aborted {
        /// Number of attempts that were executed.
        attempts: u32,
        /// Total elapsed time observed by the retry executor.
        elapsed: Duration,
        /// Failure that caused the abort.
        failure: RetryAttemptFailure<E>,
    },

    /// The maximum number of attempts has been exhausted.
    AttemptsExceeded {
        /// Number of attempts that were executed.
        attempts: u32,
        /// Configured attempt limit.
        max_attempts: u32,
        /// Total elapsed time observed by the retry executor.
        elapsed: Duration,
        /// Last observed failure.
        last_failure: RetryAttemptFailure<E>,
    },

    /// The total elapsed retry budget has been exhausted.
    MaxElapsedExceeded {
        /// Number of attempts that were executed.
        attempts: u32,
        /// Total elapsed time observed by the retry executor.
        elapsed: Duration,
        /// Configured elapsed budget.
        max_elapsed: Duration,
        /// Last failure, if any attempt ran before the budget was exhausted.
        last_failure: Option<RetryAttemptFailure<E>>,
    },
}

impl<E> RetryError<E> {
    /// Returns the number of attempts that were executed.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The number of operation attempts observed before termination.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn attempts(&self) -> u32 {
        match self {
            Self::Aborted { attempts, .. }
            | Self::AttemptsExceeded { attempts, .. }
            | Self::MaxElapsedExceeded { attempts, .. } => *attempts,
        }
    }

    /// Returns the elapsed time recorded at termination.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The elapsed duration recorded by the retry executor when it stopped.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        match self {
            Self::Aborted { elapsed, .. }
            | Self::AttemptsExceeded { elapsed, .. }
            | Self::MaxElapsedExceeded { elapsed, .. } => *elapsed,
        }
    }

    /// Returns the last failure, if one exists.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&RetryAttemptFailure<E>)` when at least one attempt failure was
    /// observed; `None` when the elapsed budget was exhausted before any
    /// attempt ran.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn last_failure(&self) -> Option<&RetryAttemptFailure<E>> {
        match self {
            Self::Aborted { failure, .. } => Some(failure),
            Self::AttemptsExceeded { last_failure, .. } => Some(last_failure),
            Self::MaxElapsedExceeded { last_failure, .. } => last_failure.as_ref(),
        }
    }

    /// Returns the last application error, if one exists.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&E)` when the terminal failure wraps an application error;
    /// `None` for timeout failures or elapsed-budget failures with no attempt.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure().and_then(RetryAttemptFailure::as_error)
    }

    /// Consumes the retry error and returns the last application error when
    /// the final failure wraps one.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(E)` when the terminal failure owns an application error; `None`
    /// when the terminal failure was a timeout or when no attempt ran.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn into_last_error(self) -> Option<E> {
        match self {
            Self::Aborted { failure, .. } => failure.into_error(),
            Self::AttemptsExceeded { last_failure, .. } => last_failure.into_error(),
            Self::MaxElapsedExceeded { last_failure, .. } => {
                last_failure.and_then(RetryAttemptFailure::into_error)
            }
        }
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
        match self {
            Self::Aborted {
                attempts,
                failure,
                ..
            } => write!(
                f,
                "retry aborted after {attempts} attempt(s); failure: {failure}"
            ),
            Self::AttemptsExceeded {
                attempts,
                max_attempts,
                last_failure,
                ..
            } => write!(
                f,
                "retry attempts exceeded: {attempts} attempt(s), max {max_attempts}; last failure: {last_failure}"
            ),
            Self::MaxElapsedExceeded {
                attempts,
                max_elapsed,
                last_failure,
                ..
            } => {
                if let Some(failure) = last_failure {
                    write!(
                        f,
                        "retry max elapsed exceeded after {attempts} attempt(s); max {max_elapsed:?}; last failure: {failure}"
                    )
                } else {
                    write!(
                        f,
                        "retry max elapsed exceeded after {attempts} attempt(s); max {max_elapsed:?}"
                    )
                }
            }
        }
    }
}

impl<E> Error for RetryError<E>
where
    E: Error + 'static,
{
    /// Returns the source application error when one is available.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&dyn Error)` when the terminal failure wraps an application error
    /// that implements [`std::error::Error`]; otherwise `None`.
    ///
    /// # Errors
    /// This method does not return errors.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error()
            .map(|error| error as &(dyn Error + 'static))
    }
}
