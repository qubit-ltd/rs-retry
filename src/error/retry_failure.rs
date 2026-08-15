// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal retry-flow failures.

use std::fmt;

use super::AttemptFailure;
use super::RetryCallbackFailure;
use super::RetryCancellationPhase;
use super::RetryInfrastructureFailure;
use super::RetryLimitKind;
use super::RetryTimeoutScope;

/// Terminal reason and retained attempt data for an unsuccessful retry flow.
///
/// Field-carrying variants cannot be constructed exhaustively outside this
/// crate. Retry executors create them while enforcing the relationship between
/// the terminal reason and the retained attempt failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum RetryFailure<E> {
    /// A retry rule deliberately stopped the flow.
    #[non_exhaustive]
    Aborted {
        /// Failure that caused the rule to abort.
        last_failure: AttemptFailure<E>,
    },
    /// A continuation limit prevented another attempt.
    #[non_exhaustive]
    Exhausted {
        /// Limit that was exhausted.
        limit: RetryLimitKind,
        /// Last attempt failure, if an attempt failed before exhaustion.
        last_failure: Option<AttemptFailure<E>>,
    },
    /// A hard timeout stopped the retry flow.
    #[non_exhaustive]
    TimedOut {
        /// Scope whose timeout expired.
        scope: RetryTimeoutScope,
        /// Last attempt failure, if one preceded the terminal timeout.
        last_failure: Option<AttemptFailure<E>>,
    },
    /// External cancellation stopped the retry flow.
    #[non_exhaustive]
    Cancelled {
        /// Retry phase in which cancellation was observed.
        phase: RetryCancellationPhase,
        /// Last attempt failure, if one preceded cancellation.
        last_failure: Option<AttemptFailure<E>>,
    },
    /// A retry callback panicked.
    #[non_exhaustive]
    CallbackFailed {
        /// Callback failure attribution.
        callback: RetryCallbackFailure,
        /// Last attempt failure, when the callback followed a failed attempt.
        last_failure: Option<AttemptFailure<E>>,
    },
    /// Retry infrastructure could not continue safely.
    #[non_exhaustive]
    Infrastructure {
        /// Infrastructure failure that stopped execution.
        failure: RetryInfrastructureFailure,
        /// Last attempt failure, if one preceded the infrastructure failure.
        last_failure: Option<AttemptFailure<E>>,
    },
}

impl<E> RetryFailure<E> {
    /// Returns the last attempt failure retained by this terminal value.
    ///
    /// # Returns
    /// `Some(&AttemptFailure<E>)` when an attempt failed before termination,
    /// or `None` when the flow stopped without a prior attempt failure.
    #[must_use]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        match self {
            Self::Aborted { last_failure } => Some(last_failure),
            Self::Exhausted { last_failure, .. }
            | Self::TimedOut { last_failure, .. }
            | Self::Cancelled { last_failure, .. }
            | Self::CallbackFailed { last_failure, .. }
            | Self::Infrastructure { last_failure, .. } => {
                last_failure.as_ref()
            }
        }
    }

    /// Returns the last application error retained by this terminal value.
    ///
    /// # Returns
    /// `Some(&E)` when the last attempt failure contains an application error,
    /// or `None` when no attempt failed or the last failure was not an
    /// application error.
    #[must_use]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure().and_then(AttemptFailure::as_error)
    }
}

impl<E: fmt::Display> fmt::Display for RetryFailure<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aborted { last_failure } => {
                write!(formatter, "retry aborted: {last_failure}")
            }
            Self::Exhausted {
                limit,
                last_failure,
            } => write_terminal(
                formatter,
                "retry limit exhausted",
                limit,
                last_failure.as_ref(),
            ),
            Self::TimedOut {
                scope,
                last_failure,
            } => write_terminal(
                formatter,
                "retry timed out",
                scope,
                last_failure.as_ref(),
            ),
            Self::Cancelled {
                phase,
                last_failure,
            } => write_terminal(
                formatter,
                "retry cancelled",
                phase,
                last_failure.as_ref(),
            ),
            Self::CallbackFailed {
                callback,
                last_failure,
            } => write_terminal(
                formatter,
                "retry callback failed",
                callback,
                last_failure.as_ref(),
            ),
            Self::Infrastructure {
                failure,
                last_failure,
            } => write_terminal(
                formatter,
                "retry infrastructure failed",
                failure,
                last_failure.as_ref(),
            ),
        }
    }
}

/// Formats a terminal classification and its optional last attempt failure.
fn write_terminal<E: fmt::Display>(
    formatter: &mut fmt::Formatter<'_>,
    label: &str,
    classification: &impl fmt::Display,
    last_failure: Option<&AttemptFailure<E>>,
) -> fmt::Result {
    write!(formatter, "{label}: {classification}")?;
    if let Some(last_failure) = last_failure {
        write!(formatter, "; last attempt failed: {last_failure}")?;
    }
    Ok(())
}
