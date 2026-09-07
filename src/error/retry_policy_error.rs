// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors returned when a retry policy contains invalid values.

use std::error::Error;
use std::fmt;

/// Invalid retry policy input.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicyError {
    /// Stable dotted configuration path identifying the invalid input.
    field: Box<str>,
    /// Owned validation reason, independent of the rejected input lifetime.
    message: Box<str>,
}

impl RetryPolicyError {
    /// Creates a policy error for one field.
    ///
    /// # Parameters
    /// - `field`: Stable configuration path, copied into the error.
    /// - `message`: Validation reason, copied into the error.
    ///
    /// # Returns
    /// An owned diagnostic that does not borrow either input.
    #[inline]
    pub(crate) fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Returns the invalid field name.
    ///
    /// # Returns
    /// Borrowed dotted configuration path identifying the rejected value.
    #[must_use]
    #[inline(always)]
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the validation message.
    ///
    /// # Returns
    /// Borrowed validation explanation, without allocating a display string.
    #[must_use]
    #[inline(always)]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RetryPolicyError {
    ///
    /// Formats the invalid configuration field and validation reason.
    ///
    /// # Parameters
    /// - `formatter`: Destination supplied by the formatting machinery.
    ///
    /// # Returns
    /// The result of writing this diagnostic representation.
    ///
    /// # Errors
    /// Returns a formatting error if the destination rejects a write.
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl Error for RetryPolicyError {}
