/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestError(pub(crate) &'static str);

impl fmt::Display for TestError {
    /// Formats the test error message.
    ///
    /// # Parameters
    /// - `f`: Formatter provided by the standard formatting machinery.
    ///
    /// # Returns
    /// `fmt::Result` from the formatter.
    ///
    /// # Errors
    /// Returns a formatting error if the underlying formatter fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for TestError {}

#[derive(Debug)]
pub(crate) struct NonCloneValue {
    pub(crate) value: String,
}
