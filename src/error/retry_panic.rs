// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable panic payload representation for retry failures.

use std::fmt;

/// Panic payload retained without exposing dynamically typed data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryPanic {
    /// A borrowed static string panic payload.
    StaticStr(
        /// Original static message, borrowed for the process lifetime.
        &'static str,
    ),
    /// An owned string panic payload.
    String(
        /// Original owned message extracted from the captured payload.
        String,
    ),
    /// A panic payload that was not a string.
    NonString,
}

impl RetryPanic {
    /// Returns the retained panic message.
    ///
    /// # Returns
    /// `Some(&str)` for static and owned string payloads, or `None` for a
    /// non-string payload.
    #[must_use]
    #[inline(always)]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::StaticStr(message) => Some(message),
            Self::String(message) => Some(message),
            Self::NonString => None,
        }
    }
}

impl fmt::Display for RetryPanic {
    ///
    /// Formats the retained string or the stable non-string payload label.
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
        match self.message() {
            Some(message) => formatter.write_str(message),
            None => formatter.write_str("non-string panic payload"),
        }
    }
}
