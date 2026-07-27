// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! `parse_display` bridge for [`std::time::Duration`] fields on
//! [`RetryDelay`](crate::RetryDelay).
//!
//! See [`RetryDelay`](crate::RetryDelay) for the full text form; this type only
//! wires each duration field to
//! [`qubit_datatype::serde::duration_millis_with_unit`].

use std::fmt;
use std::time::Duration;

use parse_display::{
    DisplayFormat,
    FromStrFormat,
    ParseError,
};
use qubit_datatype::serde::duration_millis_with_unit;

/// Bridges `parse_display` for [`Duration`] fields to
/// [`duration_millis_with_unit`].
/// `regex` returns `None` so the default non-greedy `.*?` capture is used,
/// while the adapter enforces representable whole-millisecond input.
pub(crate) struct RetryDelayDurationFormat;

impl DisplayFormat<Duration> for RetryDelayDurationFormat {
    /// Same output as [`duration_millis_with_unit::format`]: half-up rounded
    /// whole milliseconds and `ms`.
    fn write(
        &self,
        f: &mut fmt::Formatter<'_>,
        value: &Duration,
    ) -> fmt::Result {
        f.write_str(&duration_millis_with_unit::format(value))
    }
}

impl FromStrFormat<Duration> for RetryDelayDurationFormat {
    type Err = ParseError;

    /// Uses [`duration_millis_with_unit::parse`]. Structured parse errors are
    /// collapsed to a fixed [`parse_display::ParseError`] because its message
    /// is `&'static str` only.
    fn parse(&self, s: &str) -> Result<Duration, Self::Err> {
        duration_millis_with_unit::parse(s).map_err(|_| {
            ParseError::with_message(
                "invalid retry delay duration: expected a representable <integer>ms duration",
            )
        })
    }

    /// Returns no additional regular-expression constraint.
    ///
    /// # Returns
    /// Always `None`; parsing performs the complete validation.
    fn regex(&self) -> Option<String> {
        None
    }
}
