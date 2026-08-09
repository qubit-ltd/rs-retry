// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Errors returned when a retry policy contains invalid values.

use std::error::Error;
use std::fmt;

/// Invalid retry policy input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicyError {
    field: Box<str>,
    message: Box<str>,
}

impl RetryPolicyError {
    /// Creates a policy error for one field.
    pub(crate) fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Returns the invalid field name.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the validation message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RetryPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl Error for RetryPolicyError {}
