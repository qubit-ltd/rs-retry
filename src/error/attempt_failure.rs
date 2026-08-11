// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Attempt-level failure values.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::AttemptExecutionError;
use super::AttemptFailureKind;
use super::AttemptTimeoutKind;

/// Failure produced by one admitted attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(bound(
    serialize = "E: Serialize",
    deserialize = "E: DeserializeOwned"
))]
pub enum AttemptFailure<E> {
    /// The operation returned an application error.
    Error(E),
    /// The attempt was cancelled by an execution timeout.
    Timeout {
        /// Boundary that cancelled the attempt.
        kind: AttemptTimeoutKind,
    },
    /// The isolated attempt panicked.
    Panic,
    /// The executor could not complete the attempt.
    Infrastructure(AttemptExecutionError),
}

impl<E> AttemptFailure<E> {
    /// Returns the stable failure classification.
    pub fn kind(&self) -> AttemptFailureKind {
        match self {
            Self::Error(_) => AttemptFailureKind::Application,
            Self::Timeout { .. } => AttemptFailureKind::TimedOut,
            Self::Panic => AttemptFailureKind::Panicked,
            Self::Infrastructure(_) => AttemptFailureKind::Infrastructure,
        }
    }

    /// Returns whether this failure was caused by a timeout.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    /// Returns the application error, if present.
    pub fn as_error(&self) -> Option<&E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout { .. } | Self::Panic | Self::Infrastructure(_) => {
                None
            }
        }
    }

    /// Consumes the failure and returns the application error, if present.
    pub fn into_error(self) -> Option<E> {
        match self {
            Self::Error(error) => Some(error),
            Self::Timeout { .. } | Self::Panic | Self::Infrastructure(_) => {
                None
            }
        }
    }

    /// Returns the timeout kind, if this was a timeout failure.
    pub fn timeout_kind(&self) -> Option<AttemptTimeoutKind> {
        match self {
            Self::Timeout { kind } => Some(*kind),
            Self::Error(_) | Self::Panic | Self::Infrastructure(_) => None,
        }
    }

    /// Returns the executor diagnostic, if present.
    pub fn execution_error(&self) -> Option<&AttemptExecutionError> {
        match self {
            Self::Infrastructure(error) => Some(error),
            Self::Error(_) | Self::Timeout { .. } | Self::Panic => None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for AttemptFailure<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(error) => write!(formatter, "{error}"),
            Self::Timeout { kind } => {
                write!(formatter, "attempt timed out ({kind:?})")
            }
            Self::Panic => formatter.write_str("attempt panicked"),
            Self::Infrastructure(error) => {
                write!(formatter, "attempt infrastructure failed: {error}")
            }
        }
    }
}
