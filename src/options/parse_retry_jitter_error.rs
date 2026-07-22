// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry jitter parsing error.

use std::error::Error;
use std::fmt;

/// Failure to parse a [`crate::RetryJitter`] from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseRetryJitterError {
    /// Stable diagnostic describing the rejected input category.
    message: &'static str,
}

impl ParseRetryJitterError {
    /// Creates an invalid-format error.
    ///
    /// # Returns
    ///
    /// An error describing the accepted retry-jitter grammar.
    #[inline(always)]
    pub(crate) const fn invalid_format() -> Self {
        Self {
            message: "invalid retry jitter: expected 'none' or 'factor:<f64>'",
        }
    }

    /// Creates an invalid floating-point value error.
    ///
    /// # Returns
    ///
    /// An error describing the required numeric syntax.
    #[inline(always)]
    pub(crate) const fn invalid_factor() -> Self {
        Self {
            message: "invalid retry jitter factor: expected a floating-point number",
        }
    }

    /// Creates an out-of-range factor error.
    ///
    /// # Returns
    ///
    /// An error describing the supported finite factor range.
    #[inline(always)]
    pub(crate) const fn factor_out_of_range() -> Self {
        Self {
            message: "invalid retry jitter factor: expected a finite value in [0.0, 1.0]",
        }
    }
}

impl fmt::Display for ParseRetryJitterError {
    /// Formats the stable retry-jitter parse diagnostic.
    ///
    /// # Parameters
    ///
    /// * `f` - Formatter receiving the diagnostic.
    ///
    /// # Returns
    ///
    /// The formatter result.
    ///
    /// # Errors
    ///
    /// Returns an error when the formatter rejects the diagnostic.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl Error for ParseRetryJitterError {}
