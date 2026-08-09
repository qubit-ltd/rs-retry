// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal retry-flow error reasons.

use serde::Deserialize;
use serde::Serialize;

use super::retry_error_kind::RetryErrorKind;

/// Reason why the whole retry flow stopped with an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RetryErrorReason {
    /// A listener or retry policy aborted the retry flow.
    Aborted,
    /// No attempts remain.
    AttemptsExceeded,
    /// The cumulative user operation elapsed-time budget was exhausted.
    MaxOperationElapsedExceeded,
    /// The total monotonic retry-flow elapsed-time budget was exhausted.
    MaxTotalElapsedExceeded,
    /// The operation mode does not support the configured behavior.
    ///
    /// Currently used when [`Retry::run`](crate::Retry::run) receives
    /// configured per-attempt timeout options.
    UnsupportedOperation,
    /// The injected sleeper could not create or complete a retry timer.
    SleeperFailed,
    /// A timed-out blocking worker did not exit within the cancellation grace
    /// period.
    WorkerStillRunning,
    /// No attempt could continue because an explicit attempt timeout fired.
    AttemptTimedOut,
    /// The execution flow timeout fired outside a retry budget check.
    FlowTimedOut,
    /// A caller-visible retry timer failed.
    TimerFailed,
    /// Canonical replacement for an exhausted attempt budget.
    AttemptsExhausted,
    /// Canonical replacement for the operation elapsed budget.
    OperationBudgetExhausted,
    /// Canonical replacement for the total elapsed budget.
    TotalBudgetExhausted,
}

impl RetryErrorReason {
    /// Returns the stable terminal error category.
    pub fn kind(self) -> RetryErrorKind {
        match self {
            Self::Aborted => RetryErrorKind::Aborted,
            Self::AttemptsExceeded
            | Self::MaxOperationElapsedExceeded
            | Self::MaxTotalElapsedExceeded
            | Self::AttemptsExhausted
            | Self::OperationBudgetExhausted
            | Self::TotalBudgetExhausted => RetryErrorKind::Exhausted,
            Self::AttemptTimedOut | Self::FlowTimedOut => RetryErrorKind::TimedOut,
            Self::UnsupportedOperation
            | Self::SleeperFailed
            | Self::WorkerStillRunning
            | Self::TimerFailed => RetryErrorKind::Infrastructure,
        }
    }

    /// Returns whether an elapsed-time budget stopped the retry flow.
    ///
    /// # Returns
    /// `true` for operation or total elapsed-time exhaustion.
    #[inline(always)]
    pub fn is_elapsed_limit(self) -> bool {
        matches!(
            self,
            Self::MaxOperationElapsedExceeded
                | Self::MaxTotalElapsedExceeded
                | Self::OperationBudgetExhausted
                | Self::TotalBudgetExhausted
        )
    }

    /// Returns whether the retry runtime failed to provide its execution
    /// infrastructure.
    ///
    /// # Returns
    /// `true` for sleeper failures and unreaped worker failures.
    #[inline(always)]
    pub fn is_infrastructure_failure(self) -> bool {
        matches!(
            self,
            Self::SleeperFailed | Self::WorkerStillRunning | Self::TimerFailed
        )
    }

    /// Returns whether the selected execution mode cannot perform the request.
    ///
    /// # Returns
    /// `true` only for [`RetryErrorReason::UnsupportedOperation`].
    #[inline(always)]
    pub fn is_unsupported_operation(self) -> bool {
        matches!(self, Self::UnsupportedOperation)
    }
}
