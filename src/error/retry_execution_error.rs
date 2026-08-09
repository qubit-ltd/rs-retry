// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Retry-flow infrastructure failures.

use std::error::Error;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;

/// Infrastructure component that failed after or around an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryExecutionErrorKind {
    /// The configured timer or sleeper failed.
    Timer,
    /// A worker could not be safely reaped.
    Worker,
}

/// Diagnostic information for a retry execution infrastructure failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryExecutionError {
    kind: RetryExecutionErrorKind,
    message: Box<str>,
}

impl RetryExecutionError {
    /// Creates a timer failure.
    pub fn timer(message: &str) -> Self {
        Self::new(RetryExecutionErrorKind::Timer, message)
    }

    /// Creates a worker failure.
    pub fn worker(message: &str) -> Self {
        Self::new(RetryExecutionErrorKind::Worker, message)
    }

    /// Creates a failure for one infrastructure component.
    pub fn new(kind: RetryExecutionErrorKind, message: &str) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the infrastructure component.
    pub fn kind(&self) -> RetryExecutionErrorKind {
        self.kind
    }

    /// Returns the diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RetryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl fmt::Display for RetryExecutionErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Timer => "timer",
            Self::Worker => "worker",
        };
        formatter.write_str(name)
    }
}

impl Error for RetryExecutionError {}
