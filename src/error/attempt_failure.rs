/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Attempt-level failure values.
//!
//! A retry failure describes why one operation attempt did not produce a
//! successful result. It is distinct from [`crate::RetryError`], which describes
//! why the whole retry flow stopped.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::attempt_executor_error::AttemptExecutorError;
use super::attempt_panic::AttemptPanic;

/// Failure produced by a single operation attempt.
///
/// The generic parameter `E` is the caller's operation error type. Timeout,
/// panic, and executor failures do not contain `E` because they are generated
/// by the retry runtime, not returned by the operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "E: serde::Serialize",
    deserialize = "E: serde::de::DeserializeOwned"
))]
pub enum AttemptFailure<E> {
    /// The operation returned an application error.
    Error(E),

    /// The attempt exceeded the configured per-attempt timeout.
    Timeout,

    /// The attempt panicked inside an isolated execution boundary.
    Panic(AttemptPanic),

    /// The retry executor failed before the attempt could run normally.
    Executor(AttemptExecutorError),
}

impl<E> AttemptFailure<E> {
    /// Returns the application error when this failure wraps one.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&E)` for [`AttemptFailure::Error`], or `None` for
    /// runtime-generated failures.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn as_error(&self) -> Option<&E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout | Self::Panic(_) | Self::Executor(_) => None,
        }
    }

    /// Consumes the failure and returns the application error when present.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(E)` for [`AttemptFailure::Error`], or `None` for
    /// runtime-generated failures.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn into_error(self) -> Option<E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout | Self::Panic(_) | Self::Executor(_) => None,
        }
    }

    /// Returns captured panic information when this failure wraps one.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&AttemptPanic)` for [`AttemptFailure::Panic`], or `None` for other
    /// variants.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn as_panic(&self) -> Option<&AttemptPanic> {
        match self {
            Self::Panic(panic) => Some(panic),
            Self::Error(_) | Self::Timeout | Self::Executor(_) => None,
        }
    }

    /// Returns executor failure information when this failure wraps one.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// `Some(&AttemptExecutorError)` for [`AttemptFailure::Executor`], or
    /// `None` for other variants.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn as_executor_error(&self) -> Option<&AttemptExecutorError> {
        match self {
            Self::Executor(error) => Some(error),
            Self::Error(_) | Self::Timeout | Self::Panic(_) => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for AttemptFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(error) => write!(f, "{error}"),
            Self::Timeout => write!(f, "attempt timed out"),
            Self::Panic(panic) => write!(f, "attempt panicked: {panic}"),
            Self::Executor(error) => write!(f, "attempt executor failed: {error}"),
        }
    }
}
