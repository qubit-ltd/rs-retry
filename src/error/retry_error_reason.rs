// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Terminal retry-flow reasons.

use serde::Deserialize;
use serde::Serialize;

use super::RetryErrorKind;

/// Concrete reason why a retry flow stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RetryErrorReason {
    /// A rule or caller aborted the flow.
    Aborted,
    /// No attempts remain.
    AttemptsExhausted,
    /// The cumulative operation-time budget was exhausted.
    OperationBudgetExhausted,
    /// The whole-flow elapsed-time budget was exhausted.
    TotalBudgetExhausted,
    /// An admitted attempt exceeded its timeout.
    AttemptTimedOut,
    /// The execution flow timeout expired.
    FlowTimedOut,
    /// A timer or clock operation failed.
    TimerFailed,
    /// A timed-out worker did not exit within its cancellation grace period.
    WorkerStillRunning,
}

impl RetryErrorReason {
    /// Returns the stable terminal error category.
    pub fn kind(self) -> RetryErrorKind {
        match self {
            Self::Aborted => RetryErrorKind::Aborted,
            Self::AttemptsExhausted
            | Self::OperationBudgetExhausted
            | Self::TotalBudgetExhausted => RetryErrorKind::Exhausted,
            Self::AttemptTimedOut | Self::FlowTimedOut => {
                RetryErrorKind::TimedOut
            }
            Self::TimerFailed | Self::WorkerStillRunning => {
                RetryErrorKind::Infrastructure
            }
        }
    }

    /// Returns whether an elapsed budget stopped the flow.
    pub fn is_elapsed_limit(self) -> bool {
        matches!(
            self,
            Self::OperationBudgetExhausted | Self::TotalBudgetExhausted
        )
    }

    /// Returns whether infrastructure prevented continuation.
    pub fn is_infrastructure_failure(self) -> bool {
        matches!(self, Self::TimerFailed | Self::WorkerStillRunning)
    }
}
