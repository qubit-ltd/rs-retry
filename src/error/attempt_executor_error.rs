/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Attempt executor failure information.
//!

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Failure produced by the retry executor before an attempt can run normally.
///
/// This type is used for infrastructure failures such as failing to spawn a
/// worker thread for [`crate::Retry::run_in_worker`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptExecutorError {
    /// Human-readable executor failure message.
    message: Box<str>,
}

impl AttemptExecutorError {
    /// Creates an executor failure from a message.
    ///
    /// # Parameters
    /// - `message`: Failure message to store.
    ///
    /// # Returns
    /// An executor failure value.
    #[inline]
    pub fn new(message: &str) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the executor failure message.
    ///
    /// # Returns
    /// Failure message text.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AttemptExecutorError {
    /// Formats the executor failure message.
    ///
    /// # Parameters
    /// - `f`: Formatter provided by the standard formatting machinery.
    ///
    /// # Returns
    /// Formatting result.
    ///
    /// # Errors
    /// Returns a formatting error if the formatter rejects output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for AttemptExecutorError {}
