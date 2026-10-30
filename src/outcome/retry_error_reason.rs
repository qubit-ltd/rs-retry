// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal retry-flow reason values.

use std::fmt;

use crate::RetryCallbackFailure;
use crate::RetryCancellationPhase;
use crate::RetryInfrastructureFailure;
use crate::RetryLimitKind;
use crate::RetryTimeoutScope;

/// Describes why a retry flow stopped without completing successfully.
///
/// The reason records retry-control or infrastructure failures separately
/// from the application error produced by an individual attempt.
///
/// # Examples
///
/// ```
/// use qubit_retry::RetryErrorReason;
///
/// let reason = RetryErrorReason::Aborted;
/// assert_eq!(reason.to_string(), "aborted");
/// ```
#[must_use = "inspect the reason that stopped the retry flow"]
#[derive(Debug)]
#[non_exhaustive]
pub enum RetryErrorReason {
    /// A retry rule or fallback policy stopped the flow.
    Aborted,
    /// A continuation limit prevented another attempt.
    Exhausted {
        /// Continuation limit that prevented another attempt.
        limit: RetryLimitKind,
    },
    /// A hard timeout stopped the flow.
    TimedOut {
        /// Timeout scope that expired.
        scope: RetryTimeoutScope,
    },
    /// External cancellation stopped the flow.
    Cancelled {
        /// Flow phase in which cancellation was observed.
        phase: RetryCancellationPhase,
    },
    /// A retry callback failed.
    CallbackFailed {
        /// Callback failure captured at the control boundary.
        callback: RetryCallbackFailure,
    },
    /// Retry infrastructure could not continue safely.
    Infrastructure {
        /// Runtime infrastructure failure that prevented continuation.
        failure: RetryInfrastructureFailure,
    },
}

impl fmt::Display for RetryErrorReason {
    /// Formats the stable terminal-reason label and associated details.
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
            Self::Aborted => formatter.write_str("aborted"),
            Self::Exhausted { limit } => {
                write!(formatter, "exhausted ({limit})")
            }
            Self::TimedOut { scope } => {
                write!(formatter, "timed out ({scope})")
            }
            Self::Cancelled { phase } => {
                write!(formatter, "cancelled ({phase})")
            }
            Self::CallbackFailed { callback } => {
                write!(formatter, "callback failed ({callback})")
            }
            Self::Infrastructure { failure } => {
                write!(formatter, "infrastructure failure ({failure})")
            }
        }
    }
}
