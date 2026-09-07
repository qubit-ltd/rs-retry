// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Non-generic metadata for the last failed retry attempt.

use super::RetryPanic;
use super::RetryTimeoutScope;

/// Attempt classification retained after the application error is moved out.
#[derive(Debug)]
#[non_exhaustive]
pub enum AttemptFailureMetadata {
    /// The attempt returned an application error.
    ApplicationError,
    /// A hard timeout stopped the attempt.
    TimedOut {
        /// Timeout scope that terminated the attempt.
        scope: RetryTimeoutScope,
    },
    /// The isolated attempt panicked.
    Panicked {
        /// Stable representation of the panic payload.
        panic: RetryPanic,
    },
}
