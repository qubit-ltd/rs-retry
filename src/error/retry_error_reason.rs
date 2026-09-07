use std::fmt;

use super::RetryCallbackFailure;
use super::RetryCancellationPhase;
use super::RetryInfrastructureFailure;
use super::RetryLimitKind;
use super::RetryTimeoutScope;

/// Terminal reason for a retry flow, independent of its application error.
#[derive(Debug)]
#[non_exhaustive]
pub enum RetryErrorReason {
    /// A retry rule or fallback policy stopped the flow.
    Aborted,
    /// A continuation limit prevented another attempt.
    Exhausted { limit: RetryLimitKind },
    /// A hard timeout stopped the flow.
    TimedOut { scope: RetryTimeoutScope },
    /// External cancellation stopped the flow.
    Cancelled { phase: RetryCancellationPhase },
    /// A retry callback failed.
    CallbackFailed { callback: RetryCallbackFailure },
    /// Retry infrastructure could not continue safely.
    Infrastructure { failure: RetryInfrastructureFailure },
}

impl fmt::Display for RetryErrorReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aborted => formatter.write_str("aborted"),
            Self::Exhausted { limit } => write!(formatter, "exhausted ({limit})"),
            Self::TimedOut { scope } => write!(formatter, "timed out ({scope})"),
            Self::Cancelled { phase } => write!(formatter, "cancelled ({phase})"),
            Self::CallbackFailed { callback } => write!(formatter, "callback failed ({callback})"),
            Self::Infrastructure { failure } => write!(formatter, "infrastructure failure ({failure})"),
        }
    }
}
