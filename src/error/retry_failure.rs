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

#[cfg(test)]
mod tests {
    use super::RetryFailure;
    use crate::AttemptFailure;
    use crate::RetryCallbackFailure;
    use crate::RetryCallbackKind;
    use crate::RetryCallbackPhase;
    use crate::RetryCancellationPhase;
    use crate::RetryInfrastructureFailure;
    use crate::RetryLimitKind;
    use crate::RetryPanic;
    use crate::RetryTimeoutScope;
    use crate::WorkerStopTrigger;

    /// Verifies an aborted terminal value retains its required failure.
    #[test]
    fn test_aborted_carries_last_failure_and_formats_it() {
        let failure = RetryFailure::Aborted {
            last_failure: AttemptFailure::Error("aborted"),
        };

        let RetryFailure::Aborted { last_failure, .. } = &failure else {
            panic!("expected aborted failure");
        };
        assert_eq!(last_failure.as_error(), Some(&"aborted"));
        assert_eq!(failure.last_failure(), Some(last_failure));
        assert_eq!(failure.last_error(), Some(&"aborted"));
        assert_eq!(failure.to_string(), "retry aborted: aborted");
    }

    /// Verifies an exhausted terminal value retains its limit and last error.
    #[test]
    fn test_exhausted_carries_limit_and_last_failure() {
        let failure = RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            last_failure: Some(AttemptFailure::Error("exhausted")),
        };

        let RetryFailure::Exhausted {
            limit,
            last_failure,
            ..
        } = &failure
        else {
            panic!("expected exhausted failure");
        };
        assert_eq!(*limit, RetryLimitKind::Attempts);
        assert_eq!(failure.last_failure(), last_failure.as_ref());
        assert_eq!(failure.last_error(), Some(&"exhausted"));
        assert_eq!(
            failure.to_string(),
            "retry limit exhausted: attempts; last attempt failed: exhausted"
        );
    }

    /// Verifies a timed-out terminal value retains its timeout scope.
    #[test]
    fn test_timed_out_carries_scope_without_last_failure() {
        let failure: RetryFailure<&str> = RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            last_failure: None,
        };

        let RetryFailure::TimedOut {
            scope,
            last_failure,
            ..
        } = &failure
        else {
            panic!("expected timed-out failure");
        };
        assert_eq!(*scope, RetryTimeoutScope::Flow);
        assert_eq!(last_failure, &None);
        assert_eq!(failure.last_failure(), None);
        assert_eq!(failure.last_error(), None);
        assert_eq!(failure.to_string(), "retry timed out: flow");
    }

    /// Verifies a cancelled terminal value retains its phase and last error.
    #[test]
    fn test_cancelled_carries_phase_and_last_failure() {
        let failure = RetryFailure::Cancelled {
            phase: RetryCancellationPhase::Backoff,
            last_failure: Some(AttemptFailure::Error("cancelled")),
        };

        let RetryFailure::Cancelled {
            phase,
            last_failure,
            ..
        } = &failure
        else {
            panic!("expected cancelled failure");
        };
        assert_eq!(*phase, RetryCancellationPhase::Backoff);
        assert_eq!(failure.last_failure(), last_failure.as_ref());
        assert_eq!(failure.last_error(), Some(&"cancelled"));
        assert_eq!(
            failure.to_string(),
            "retry cancelled: backoff; last attempt failed: cancelled"
        );
    }

    /// Verifies a callback terminal value retains callback attribution.
    #[test]
    fn test_callback_failed_carries_callback_and_last_failure() {
        let failure: RetryFailure<&str> = RetryFailure::CallbackFailed {
            callback: RetryCallbackFailure::new(
                RetryCallbackKind::Observer,
                3,
                RetryCallbackPhase::RetryScheduled,
                RetryPanic::StaticStr("observer panic"),
            ),
            last_failure: None,
        };

        let RetryFailure::CallbackFailed {
            callback,
            last_failure,
            ..
        } = &failure
        else {
            panic!("expected callback failure");
        };
        assert_eq!(callback.callback(), RetryCallbackKind::Observer);
        assert_eq!(callback.index(), 3);
        assert_eq!(last_failure, &None);
        assert_eq!(failure.last_failure(), None);
        assert_eq!(failure.last_error(), None);
        assert_eq!(
            failure.to_string(),
            "retry callback failed: observer callback 3 panicked during retry scheduled: observer panic"
        );
    }

    /// Verifies an infrastructure terminal value retains failure attribution.
    #[test]
    fn test_infrastructure_carries_failure_and_last_error() {
        let failure = RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::AttemptTimeout,
            },
            last_failure: Some(AttemptFailure::Error("worker failed")),
        };

        let RetryFailure::Infrastructure {
            failure: infrastructure,
            last_failure,
            ..
        } = &failure
        else {
            panic!("expected infrastructure failure");
        };
        assert_eq!(
            infrastructure.worker_stop_trigger(),
            Some(WorkerStopTrigger::AttemptTimeout)
        );
        assert_eq!(failure.last_failure(), last_failure.as_ref());
        assert_eq!(failure.last_error(), Some(&"worker failed"));
        assert_eq!(
            failure.to_string(),
            "retry infrastructure failed: worker still running after attempt timeout; last attempt failed: worker failed"
        );
    }
}
