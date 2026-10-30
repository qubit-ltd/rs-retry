// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry callback classifications.

use std::fmt;

/// Callback category that failed while controlling a retry flow.
///
/// This classifies which callback failed. The terminal reason describes why
/// the flow stopped, while completion callback diagnostics retain failures
/// that occurred after the terminal state was frozen.
///
/// # Examples
/// ```
/// use qubit_retry::RetryCallbackKind;
///
/// let kind = RetryCallbackKind::Observer;
/// assert!(matches!(kind, RetryCallbackKind::Observer));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryCallbackKind {
    /// A retry rule callback.
    Rule,
    /// A retry observer callback.
    Observer,
}

impl fmt::Display for RetryCallbackKind {
    /// Formats the stable human-readable label of this classification.
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
        let name = match self {
            Self::Rule => "rule",
            Self::Observer => "observer",
        };
        formatter.write_str(name)
    }
}
