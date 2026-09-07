// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Attempt-level failure values.

use std::fmt;

use super::RetryPanic;
use super::RetryTimeoutScope;

/// Failure produced by one admitted attempt.
///
/// # Type Parameters
/// - `E`: Owned application error; timeout and panic variants do not contain
///   it.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttemptFailure<E> {
    /// The operation returned an application error.
    Error(
        /// Original application error, retained without conversion.
        E,
    ),
    /// A hard timeout stopped the attempt.
    TimedOut {
        /// Scope whose timeout expired.
        scope: RetryTimeoutScope,
    },
    /// The isolated attempt panicked.
    Panicked {
        /// Stable representation of the panic payload.
        panic: RetryPanic,
    },
}

impl<E> AttemptFailure<E> {
    /// Returns whether this failure was caused by a timeout.
    ///
    /// # Returns
    /// True for a captured hard timeout, false for application error or panic.
    #[must_use]
    #[inline(always)]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::TimedOut { .. })
    }

    /// Returns the application error, if present.
    ///
    /// # Returns
    /// `Some(&E)` for [`Self::Error`], or `None` for timeout and panic
    /// failures.
    #[must_use]
    #[inline(always)]
    pub fn as_error(&self) -> Option<&E> {
        match self {
            Self::Error(error) => Some(error),
            Self::TimedOut { .. } | Self::Panicked { .. } => None,
        }
    }

    /// Returns the timeout scope, if this failure was caused by a timeout.
    ///
    /// # Returns
    /// `Some(RetryTimeoutScope)` for [`Self::TimedOut`], or `None` otherwise.
    #[must_use]
    #[inline(always)]
    pub fn timeout_scope(&self) -> Option<RetryTimeoutScope> {
        match self {
            Self::TimedOut { scope } => Some(*scope),
            Self::Error(_) | Self::Panicked { .. } => None,
        }
    }

    /// Returns the captured panic payload, if the attempt panicked.
    ///
    /// # Returns
    /// `Some(&RetryPanic)` for [`Self::Panicked`], or `None` otherwise.
    #[must_use]
    #[inline(always)]
    pub fn panic(&self) -> Option<&RetryPanic> {
        match self {
            Self::Panicked { panic } => Some(panic),
            Self::Error(_) | Self::TimedOut { .. } => None,
        }
    }

    /// Maps the contained application error while preserving other failures.
    ///
    /// The mapper is called exactly once for [`Self::Error`] and is not called
    /// for timeout or panic failures. A mapper panic propagates to the caller.
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
    pub fn map_error<U, F: FnOnce(E) -> U>(self, map: F) -> AttemptFailure<U> {
        match self {
            Self::Error(error) => AttemptFailure::Error(map(error)),
            Self::TimedOut { scope } => AttemptFailure::TimedOut { scope },
            Self::Panicked { panic } => AttemptFailure::Panicked { panic },
        }
    }

    /// Consumes the failure and returns the application error, if present.
    ///
    /// # Returns
    /// `Some(E)` for [`Self::Error`], or `None` for timeout and panic failures.
    #[must_use]
    #[inline]
    pub fn into_error(self) -> Option<E> {
        match self {
            Self::Error(error) => Some(error),
            Self::TimedOut { .. } | Self::Panicked { .. } => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for AttemptFailure<E> {
    ///
    /// Formats the application error, timeout scope, or captured operation
    /// panic.
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
        match self {
            Self::Error(error) => write!(formatter, "{error}"),
            Self::TimedOut { scope } => {
                write!(formatter, "attempt timed out ({scope})")
            }
            Self::Panicked { panic } => {
                write!(formatter, "attempt panicked: {panic}")
            }
        }
    }
}
