// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured failures from retry continuation accounting.

use qubit_clock::TimeError;
use thiserror::Error;

use super::RetryBudgetExhausted;

/// A clock failure, invalid attempt token, or normal continuation exhaustion.
#[must_use]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RetryBudgetError {
    /// The clock changed domains or moved backward.
    #[error("retry budget clock failed: {0}")]
    Clock(
        /// Underlying clock-domain or monotonicity failure.
        #[from]
        TimeError,
    ),
    /// A continuation limit prevents further work.
    #[error("retry budget exhausted: {0:?}")]
    Exhausted(
        /// First continuation limit that prohibits another operation.
        RetryBudgetExhausted,
    ),
    /// An admitted operation must finish before another one can start.
    #[error("retry budget already has an active attempt")]
    AttemptInProgress,
    /// The token belongs to another budget or no longer names the active
    /// attempt.
    #[error("retry attempt does not belong to the active budget operation")]
    InvalidAttempt,
}
