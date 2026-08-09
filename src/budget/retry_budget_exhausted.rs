// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Defines normal retry continuation-budget exhaustion.

/// The continuation budget that prevents another retry action.
///
/// This is not an execution error: a currently running attempt is never
/// cancelled by this value, and a successful completed attempt always wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryBudgetExhausted {
    /// No further attempts may be admitted.
    Attempts,
    /// The cumulative operation duration cannot admit another attempt.
    OperationElapsed,
    /// The end-to-end monotonic deadline cannot admit another action.
    TotalElapsed,
}
