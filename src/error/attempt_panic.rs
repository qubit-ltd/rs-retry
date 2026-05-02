/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Captured attempt panic information.
//!

use std::any::Any;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Panic payload captured from an isolated attempt.
///
/// `AttemptPanic` stores a best-effort text message extracted from the panic
/// payload. String payloads and `&'static str` payloads preserve their original
/// text; all other payload types use a generic message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptPanic {
    /// Human-readable panic message.
    message: Box<str>,
}

impl AttemptPanic {
    /// Creates captured panic information from a message.
    ///
    /// # Parameters
    /// - `message`: Panic message to store.
    ///
    /// # Returns
    /// A captured panic value.
    #[inline]
    pub fn new(message: &str) -> Self {
        Self::from_string(message.to_string())
    }

    /// Creates captured panic information from an owned message.
    ///
    /// # Parameters
    /// - `message`: Panic message to store.
    ///
    /// # Returns
    /// A captured panic value.
    #[inline]
    pub(crate) fn from_string(message: String) -> Self {
        Self {
            message: message.into_boxed_str(),
        }
    }

    /// Extracts captured panic information from a panic payload.
    ///
    /// # Parameters
    /// - `payload`: Payload returned by `catch_unwind`.
    ///
    /// # Returns
    /// A captured panic value with best-effort text.
    pub(crate) fn from_payload(payload: Box<dyn Any + Send + 'static>) -> Self {
        match payload.downcast::<String>() {
            Ok(message) => Self::from_string(*message),
            Err(payload) => match payload.downcast::<&'static str>() {
                Ok(message) => Self::new(*message),
                Err(_) => Self::new("attempt panicked with a non-string payload"),
            },
        }
    }

    /// Returns the captured panic message.
    ///
    /// # Returns
    /// Panic message text.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AttemptPanic {
    /// Formats the captured panic message.
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

impl Error for AttemptPanic {}
