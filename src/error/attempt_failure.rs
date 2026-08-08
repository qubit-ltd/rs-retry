// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Attempt-level failure values.
//!
//! A retry failure describes why one operation attempt did not produce a
//! successful result. It is distinct from [`crate::RetryError`], which
//! describes why the whole retry flow stopped.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;

use super::attempt_executor_error::AttemptExecutorError;
use super::attempt_failure_kind::AttemptFailureKind;
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
    Error(
        /// Application error returned by the operation.
        E,
    ),

    /// The attempt exceeded the effective timeout.
    ///
    /// This can be the configured per-attempt timeout, the remaining
    /// max-operation-elapsed budget, or the remaining max-total-elapsed budget
    /// used by async and worker-thread attempts.
    Timeout,

    /// The attempt panicked inside an isolated execution boundary.
    Panic(
        /// Captured panic information.
        AttemptPanic,
    ),

    /// The retry executor failed before the attempt could run normally.
    Executor(
        /// Retry executor failure details.
        AttemptExecutorError,
    ),
}

impl<E> AttemptFailure<E> {
    /// Returns the stable semantic kind of this failure.
    ///
    /// # Returns
    /// The variant-independent failure classification.
    #[inline(always)]
    pub fn kind(&self) -> AttemptFailureKind {
        match self {
            Self::Error(_) => AttemptFailureKind::Error,
            Self::Timeout => AttemptFailureKind::Timeout,
            Self::Panic(_) => AttemptFailureKind::Panic,
            Self::Executor(_) => AttemptFailureKind::Executor,
        }
    }

    /// Returns whether this failure represents an effective timeout.
    ///
    /// # Returns
    /// `true` only for [`AttemptFailure::Timeout`].
    #[inline(always)]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout)
    }

    /// Returns the application error when this failure wraps one.
    ///
    /// # Returns
    /// `Some(&E)` for [`AttemptFailure::Error`], or `None` for
    /// runtime-generated failures.
    #[inline(always)]
    pub fn as_error(&self) -> Option<&E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout | Self::Panic(_) | Self::Executor(_) => None,
        }
    }

    /// Consumes the failure and returns the application error when present.
    ///
    /// # Returns
    /// `Some(E)` for [`AttemptFailure::Error`], or `None` for
    /// runtime-generated failures.
    #[inline(always)]
    pub fn into_error(self) -> Option<E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout | Self::Panic(_) | Self::Executor(_) => None,
        }
    }

    /// Returns captured panic information when this failure wraps one.
    ///
    /// # Returns
    /// `Some(&AttemptPanic)` for [`AttemptFailure::Panic`], or `None` for other
    /// variants.
    #[inline(always)]
    pub fn as_panic(&self) -> Option<&AttemptPanic> {
        match self {
            Self::Panic(panic) => Some(panic),
            Self::Error(_) | Self::Timeout | Self::Executor(_) => None,
        }
    }

    /// Returns executor failure information when this failure wraps one.
    ///
    /// # Returns
    /// `Some(&AttemptExecutorError)` for [`AttemptFailure::Executor`], or
    /// `None` for other variants.
    #[inline(always)]
    pub fn as_executor_error(&self) -> Option<&AttemptExecutorError> {
        match self {
            Self::Executor(error) => Some(error),
            Self::Error(_) | Self::Timeout | Self::Panic(_) => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for AttemptFailure<E> {
    /// Formats this attempt failure.
    ///
    /// # Parameters
    /// - `f`: Destination formatter.
    ///
    /// # Returns
    /// [`fmt::Result`] indicating whether formatting succeeded.
    ///
    /// # Errors
    /// Returns [`fmt::Error`] if writing to the formatter fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(error) => write!(f, "{error}"),
            Self::Timeout => write!(f, "attempt timed out"),
            Self::Panic(panic) => write!(f, "attempt panicked: {panic}"),
            Self::Executor(error) => {
                write!(f, "attempt executor failed: {error}")
            }
        }
    }
}
