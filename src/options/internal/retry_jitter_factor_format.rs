// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Text formatting and validated parsing for retry jitter factors.

use parse_display::{
    DisplayFormat,
    FromStrFormat,
    ParseError,
};

/// Formats jitter factors as `f64` text and parses them with range validation.
pub(in crate::options) struct RetryJitterFactorFormat;

impl DisplayFormat<f64> for RetryJitterFactorFormat {
    /// Writes the factor using the default `f64` formatter.
    ///
    /// # Arguments
    /// - `f`: Output formatter.
    /// - `value`: Factor value.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// Returns [`std::fmt::Error`] if the formatter rejects output.
    fn write(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        value: &f64,
    ) -> std::fmt::Result {
        write!(f, "{value}")
    }
}

impl FromStrFormat<f64> for RetryJitterFactorFormat {
    /// Error returned by factor parsing.
    type Err = ParseError;

    /// Parses and validates a factor in range `[0.0, 1.0]`.
    ///
    /// # Arguments
    /// - `s`: Raw factor text captured by `parse-display`.
    ///
    /// # Returns
    /// The parsed factor.
    ///
    /// # Errors
    /// Returns [`ParseError`] when the input is not a valid `f64` or lies
    /// outside `[0.0, 1.0]`, including non-finite values.
    fn parse(&self, s: &str) -> Result<f64, Self::Err> {
        let value = s.parse::<f64>().map_err(|_| {
            ParseError::with_message("invalid retry jitter factor")
        })?;
        if !(0.0..=1.0).contains(&value) {
            return Err(ParseError::with_message(
                "retry jitter factor must be in range [0.0, 1.0]",
            ));
        }
        Ok(value)
    }
}
