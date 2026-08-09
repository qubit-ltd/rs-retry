// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Attempt-level executor failures.

use std::error::Error;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;

/// Failure before an attempt could produce an application result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptExecutionError {
    message: Box<str>,
}

impl AttemptExecutionError {
    /// Creates an attempt execution failure.
    pub fn new(message: &str) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the execution diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AttemptExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AttemptExecutionError {}
