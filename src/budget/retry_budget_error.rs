// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Errors that prevent construction of a retry budget.

use qubit_clock::TimeError;
use thiserror::Error;

/// A failure while constructing retry continuation budgets.
#[derive(Debug, Error)]
pub enum RetryBudgetError {
    /// The clock could not represent the configured total elapsed deadline.
    #[error("retry total elapsed deadline cannot be represented: {0}")]
    Clock(#[source] TimeError),
}
