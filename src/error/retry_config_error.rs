// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration validation errors.
//!
//! This module keeps retry configuration failures independent from executor
//! execution failures so callers can distinguish setup errors from operation
//! errors.

use std::error::Error;
use std::fmt;

use qubit_argument::{
    ArgumentError,
    ArgumentErrorKind,
};

#[cfg(feature = "config")]
use qubit_config::ConfigError;

/// Invalid retry configuration.
///
/// `path` stores the configuration key that failed when such context is
/// available. `message` stores the human-readable reason. Structured argument
/// failures are retained so their diagnostic can be rendered without
/// duplicating the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryConfigError {
    path: String,
    message: String,
    argument_source: Option<ArgumentError>,
}

impl RetryConfigError {
    /// Creates a validation error for a retry option.
    ///
    /// # Parameters
    /// - `path`: Configuration key or option name associated with the failure.
    /// - `message`: Human-readable validation message.
    ///
    /// # Returns
    /// A new [`RetryConfigError`].
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    pub fn invalid_value(
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            argument_source: None,
        }
    }

    /// Wraps an error returned by `qubit-config`.
    ///
    /// # Parameters
    /// - `path`: Configuration key that was being read.
    /// - `source`: Error returned by `qubit-config`.
    ///
    /// # Returns
    /// A new [`RetryConfigError`] that preserves the key and source message.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    #[cfg(feature = "config")]
    pub fn from_config(path: impl Into<String>, source: ConfigError) -> Self {
        Self {
            path: path.into(),
            message: source.to_string(),
            argument_source: None,
        }
    }

    /// Returns the configuration path associated with this error.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The configuration path, or an empty string when the error was not tied
    /// to a specific key.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the error message.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The human-readable validation or configuration read message.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RetryConfigError {
    /// Formats the configuration error for diagnostics.
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
        if let Some(source) = &self.argument_source {
            write!(f, "invalid retry configuration: {source}")
        } else if self.path.is_empty() {
            write!(f, "invalid retry configuration: {}", self.message)
        } else {
            write!(
                f,
                "invalid retry configuration at '{}': {}",
                self.path, self.message
            )
        }
    }
}

impl Error for RetryConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.argument_source
            .as_ref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

/// Extracts the compatibility message from an argument validation error.
///
/// Custom validators carry the legacy public message directly. Other error
/// kinds fall back to their complete structured diagnostic.
///
/// # Parameters
/// - `error`: Structured argument validation error.
///
/// # Returns
/// The legacy custom message or complete structured diagnostic.
pub(crate) fn argument_error_message(error: ArgumentError) -> String {
    match error.kind() {
        ArgumentErrorKind::Custom { message, .. } => message.clone(),
        _ => error.to_string(),
    }
}

impl From<ArgumentError> for RetryConfigError {
    /// Converts a structured argument failure into a retry configuration error.
    ///
    /// Custom validation messages are preserved without the argument display
    /// prefix so existing retry diagnostics remain stable.
    ///
    /// # Parameters
    /// - `source`: Structured argument validation error.
    ///
    /// # Returns
    /// A retry configuration error preserving the validation path and message.
    ///
    /// # Errors
    /// This function does not return errors.
    fn from(source: ArgumentError) -> Self {
        let path = source.path().as_str().to_owned();
        if matches!(source.kind(), ArgumentErrorKind::Custom { .. }) {
            let message = argument_error_message(source);
            Self::invalid_value(path, message)
        } else {
            let message = source.to_string();
            Self {
                path,
                message,
                argument_source: Some(source),
            }
        }
    }
}

#[cfg(feature = "config")]
impl From<ConfigError> for RetryConfigError {
    /// Converts a `qubit-config` error into a retry configuration error.
    ///
    /// # Parameters
    /// - `source`: Error returned by `qubit-config`.
    ///
    /// # Returns
    /// A [`RetryConfigError`] with the path carried by `source` when
    /// available, or an empty path for config errors that do not include key
    /// context.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    fn from(source: ConfigError) -> Self {
        let path = match &source {
            ConfigError::PropertyNotFound(path)
            | ConfigError::PropertyHasNoValue(path)
            | ConfigError::PropertyIsFinal(path) => path.clone(),
            ConfigError::TypeMismatch { key, .. }
            | ConfigError::ConversionError { key, .. } => key.clone(),
            ConfigError::DeserializeError { path, .. } => path.clone(),
            ConfigError::KeyConflict { path, .. } => path.clone(),
            ConfigError::SubstitutionError(_)
            | ConfigError::SubstitutionDepthExceeded(_)
            | ConfigError::SubstitutionCycle { .. }
            | ConfigError::MergeError(_)
            | ConfigError::IoError(_)
            | ConfigError::ParseError(_)
            | ConfigError::Other(_) => String::new(),
        };
        Self::from_config(path, source)
    }
}
