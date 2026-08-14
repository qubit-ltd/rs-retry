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
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttemptFailure<E> {
    /// The operation returned an application error.
    Error(E),
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
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::TimedOut { .. })
    }

    /// Returns the application error, if present.
    ///
    /// # Returns
    /// `Some(&E)` for [`Self::Error`], or `None` for timeout and panic
    /// failures.
    #[must_use]
    pub fn as_error(&self) -> Option<&E> {
        match self {
            Self::Error(error) => Some(error),
            Self::TimedOut { .. } | Self::Panicked { .. } => None,
        }
    }

    /// Consumes the failure and returns the application error, if present.
    ///
    /// # Returns
    /// `Some(E)` for [`Self::Error`], or `None` for timeout and panic failures.
    #[must_use]
    pub fn into_error(self) -> Option<E> {
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
    pub fn panic(&self) -> Option<&RetryPanic> {
        match self {
            Self::Panicked { panic } => Some(panic),
            Self::Error(_) | Self::TimedOut { .. } => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for AttemptFailure<E> {
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
