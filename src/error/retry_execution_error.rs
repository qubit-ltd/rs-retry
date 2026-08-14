// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry-flow infrastructure failures.

use std::error::Error;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;

use super::retry_execution_error_kind::RetryExecutionErrorKind;

/// Diagnostic information for a retry execution infrastructure failure.
#[must_use]
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
    #[must_use]
    pub fn kind(&self) -> RetryExecutionErrorKind {
        self.kind
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RetryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl Error for RetryExecutionError {}
