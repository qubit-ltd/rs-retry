// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! RetryDelay strategies for retry attempts.
//!
//! A [`RetryDelay`] produces the base sleep duration after a failed attempt.
//! The base duration is calculated before [`crate::RetryJitter`] is applied by
//! a retry executor.
//!
//! # Text interchange
//!
//! [`std::fmt::Display`] and [`std::str::FromStr`] share a canonical string
//! form:
//!
//! - `none`
//! - `fixed(<duration>)` — duration fields are displayed as half-up rounded
//!   whole milliseconds with an `ms` suffix; `FromStr` accepts any duration
//!   string parsed by [`qubit_serde::serde::duration_millis_with_unit`]
//! - `random(<min>..=<max>)` — same rules for the two duration fields
//! - `exponential(initial=<...>, max=<...>, multiplier=<f64>)` — same for
//!   `initial` and `max`
//!
//! For [`std::str::FromStr`], substrings for duration fields follow
//! [`qubit_serde::serde::duration_millis_with_unit`] (bare integer as
//! milliseconds, unit suffixes, etc.; see that module).
//! [`std::fmt::Display`] normalizes to whole millisecond + `ms` for those
//! fields.

use std::str::FromStr;
use std::time::Duration;

use parse_display::{Display, FromStr};
use qubit_argument::{ArgumentResult, require_that};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::retry_delay_duration_format::RetryDelayDurationFormat;
use crate::constants::DEFAULT_RETRY_DELAY;
use crate::error::argument_error_message;

/// Base delay strategy before jitter is applied.
///
/// RetryDelay strategies are value types that can be reused across executors.
/// Random and exponential strategies are validated separately by
/// [`RetryDelay::validate`], which is called when building
/// [`crate::RetryOptions`].
#[derive(Debug, Clone, PartialEq, Display, FromStr, Serialize, Deserialize)]
pub enum RetryDelay {
    /// Retry immediately.
    #[display("none")]
    None,

    /// Wait for a constant delay after every failed attempt.
    #[display("fixed({0})")]
    Fixed(
        #[display(with = RetryDelayDurationFormat)]
        #[serde(with = "qubit_serde::serde::duration_millis")]
        Duration,
    ),

    /// Pick a delay uniformly from the inclusive range.
    #[display("random({min}..={max})")]
    Random {
        /// Lower bound for the delay.
        #[display(with = RetryDelayDurationFormat)]
        #[serde(with = "qubit_serde::serde::duration_millis")]
        min: Duration,
        /// Upper bound for the delay.
        #[display(with = RetryDelayDurationFormat)]
        #[serde(with = "qubit_serde::serde::duration_millis")]
        max: Duration,
    },

    /// Exponential backoff capped by `max`.
    #[display("exponential(initial={initial}, max={max}, multiplier={multiplier})")]
    Exponential {
        /// RetryDelay used for the first retry.
        #[display(with = RetryDelayDurationFormat)]
        #[serde(with = "qubit_serde::serde::duration_millis")]
        initial: Duration,
        /// Maximum delay.
        #[display(with = RetryDelayDurationFormat)]
        #[serde(with = "qubit_serde::serde::duration_millis")]
        max: Duration,
        /// Multiplicative factor applied per failed attempt.
        multiplier: f64,
    },
}

impl RetryDelay {
    /// Creates a no-delay strategy.
    ///
    /// # Returns
    /// A [`RetryDelay::None`] strategy.
    #[inline(always)]
    pub fn none() -> Self {
        Self::None
    }

    /// Creates a fixed-delay strategy.
    ///
    /// This constructor stores `delay` without validating it; call
    /// [`RetryDelay::validate`] to reject a zero duration.
    ///
    /// # Arguments
    /// - `delay`: Duration slept after each failed attempt.
    ///
    /// # Returns
    /// A [`RetryDelay::Fixed`] strategy.
    #[inline(always)]
    pub fn fixed(delay: Duration) -> Self {
        Self::Fixed(delay)
    }

    /// Creates a random-delay strategy.
    ///
    /// This constructor stores the range without validating it; call
    /// [`RetryDelay::validate`] before using caller-supplied bounds.
    ///
    /// # Arguments
    /// - `min`: Inclusive lower bound for generated delays.
    /// - `max`: Inclusive upper bound for generated delays.
    ///
    /// # Returns
    /// A [`RetryDelay::Random`] strategy.
    #[inline(always)]
    pub fn random(min: Duration, max: Duration) -> Self {
        Self::Random { min, max }
    }

    /// Creates an exponential-backoff strategy.
    ///
    /// This constructor stores the parameters without validating them; call
    /// [`RetryDelay::validate`] before using caller-supplied values.
    ///
    /// # Arguments
    /// - `initial`: RetryDelay used for the first retry.
    /// - `max`: Upper bound applied to every calculated delay.
    /// - `multiplier`: Factor applied for each subsequent failed attempt.
    ///
    /// # Returns
    /// A [`RetryDelay::Exponential`] strategy.
    #[inline(always)]
    pub fn exponential(initial: Duration, max: Duration, multiplier: f64) -> Self {
        Self::Exponential {
            initial,
            max,
            multiplier,
        }
    }

    /// Calculates the base delay for an attempt number starting at 1.
    ///
    /// Attempt `1` means the first failed attempt, so exponential backoff
    /// returns `initial` for attempts `0` and `1`. Random delays use a fresh
    /// random value for every call.
    /// Caller-supplied strategies should be checked with
    /// [`RetryDelay::validate`] before execution.
    ///
    /// # Arguments
    /// - `attempt`: Failed attempt number. Values `0` and `1` are treated as
    ///   the first exponential-backoff step.
    ///
    /// # Returns
    /// The base delay before jitter is applied.
    pub fn base_delay(&self, attempt: u32) -> Duration {
        match self {
            Self::None => Duration::ZERO,
            Self::Fixed(delay) => *delay,
            Self::Random { min, max } => {
                if min >= max {
                    return *min;
                }
                let mut rng = rand::rng();
                let min_nanos = Self::duration_to_nanos_u64(*min);
                let max_nanos = Self::duration_to_nanos_u64(*max);
                Duration::from_nanos(rng.random_range(min_nanos..=max_nanos))
            }
            Self::Exponential {
                initial,
                max,
                multiplier,
            } => Self::exponential_delay(*initial, *max, *multiplier, attempt),
        }
    }

    /// Validates strategy parameters.
    ///
    /// Returns a human-readable message describing the invalid field when the
    /// strategy cannot be used safely by an executor.
    ///
    /// # Returns
    /// `Ok(())` when all parameters are usable; otherwise an error message that
    /// can be wrapped by [`crate::RetryConfigError`].
    ///
    /// # Errors
    /// Returns an error when a fixed delay is zero, a random range is invalid,
    /// random bounds cannot be sampled as `u64` nanoseconds, or exponential
    /// backoff parameters are zero, inverted, non-finite, or too small.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_argument("delay")
            .map_err(argument_error_message)
    }

    /// Validates strategy parameters with structured argument error context.
    ///
    /// # Arguments
    /// - `path`: Configuration path associated with the delay strategy.
    ///
    /// # Returns
    /// `Ok(())` when the delay strategy is usable.
    ///
    /// # Errors
    /// Returns an argument error at `path` when the selected strategy cannot
    /// be used safely by an executor.
    pub(super) fn validate_argument(&self, path: &str) -> ArgumentResult<()> {
        match self {
            Self::None => Ok(()),
            Self::Fixed(delay) => {
                require_that(
                    *delay,
                    path,
                    |delay| !delay.is_zero(),
                    "fixed_delay_positive",
                    "fixed delay cannot be zero",
                )?;
                Ok(())
            }
            Self::Random { min, max } => {
                require_that(
                    *min,
                    path,
                    |min| !min.is_zero(),
                    "random_delay_minimum_positive",
                    "random delay minimum cannot be zero",
                )?;
                require_that(
                    (*min, *max),
                    path,
                    |(min, max)| min <= max,
                    "random_delay_order",
                    "random delay minimum cannot be greater than maximum",
                )?;
                require_that(
                    (*min, *max),
                    path,
                    |(min, max)| {
                        Self::duration_fits_nanos_u64(*min) && Self::duration_fits_nanos_u64(*max)
                    },
                    "random_delay_nanos_range",
                    "random delay bounds must fit into u64 nanoseconds",
                )?;
                Ok(())
            }
            Self::Exponential {
                initial,
                max,
                multiplier,
            } => {
                require_that(
                    *initial,
                    path,
                    |initial| !initial.is_zero(),
                    "exponential_delay_initial_positive",
                    "exponential delay initial value cannot be zero",
                )?;
                require_that(
                    (*initial, *max),
                    path,
                    |(initial, max)| max >= initial,
                    "exponential_delay_order",
                    "exponential delay maximum cannot be smaller than initial",
                )?;
                require_that(
                    *multiplier,
                    path,
                    |multiplier| multiplier.is_finite() && *multiplier > 1.0,
                    "exponential_delay_multiplier",
                    "exponential delay multiplier must be finite and greater than 1.0",
                )?;
                Ok(())
            }
        }
    }
    /// Returns whether a duration can be represented as whole nanoseconds in
    /// `u64`.
    ///
    /// # Arguments
    /// - `duration`: Duration to inspect.
    ///
    /// # Returns
    /// `true` when the duration can be sampled by the random delay generator
    /// without lossy saturation.
    fn duration_fits_nanos_u64(duration: Duration) -> bool {
        duration.as_nanos() <= u64::MAX as u128
    }

    /// Converts a [`Duration`] to whole nanoseconds as `u64`.
    ///
    /// Values larger than [`u64::MAX`] nanoseconds are saturated to
    /// [`u64::MAX`] so the result fits in `u64` for uniform random delay
    /// sampling in [`RetryDelay::base_delay`].
    ///
    /// # Arguments
    /// - `duration`: Duration to convert.
    ///
    /// # Returns
    /// The duration in nanoseconds, capped at [`u64::MAX`].
    fn duration_to_nanos_u64(duration: Duration) -> u64 {
        duration.as_nanos().min(u64::MAX as u128) as u64
    }

    /// Computes the exponential backoff delay for a given failed-attempt index.
    ///
    /// The effective exponent is `attempt.saturating_sub(1)`, so attempts `0`
    /// and `1` both yield the initial delay (matching
    /// [`RetryDelay::base_delay`]). Each further attempt multiplies the
    /// base nanosecond count by `multiplier` that many times, then the
    /// result is capped at `max`.
    ///
    /// # Arguments
    /// - `initial`: RetryDelay for the first retry step (attempts `0` and `1`).
    /// - `max`: Upper bound on the returned delay.
    /// - `multiplier`: Factor applied per additional attempt beyond the first.
    /// - `attempt`: Failed attempt number (see [`RetryDelay::base_delay`]).
    ///
    /// # Returns
    /// The computed delay, or `max` when the scaled value is not finite or is
    /// not less than `max` in nanoseconds.
    fn exponential_delay(
        initial: Duration,
        max: Duration,
        multiplier: f64,
        attempt: u32,
    ) -> Duration {
        let power = attempt.saturating_sub(1);
        let factor = multiplier.powi(power.min(i32::MAX as u32) as i32);
        if !factor.is_finite() {
            return max;
        }
        let secs = initial.as_secs_f64() * factor;
        if !secs.is_finite() || secs >= max.as_secs_f64() {
            return max;
        }
        Duration::try_from_secs_f64(secs).map_or(max, |delay| delay.min(max))
    }
}

impl Default for RetryDelay {
    /// Creates the default exponential-backoff strategy.
    ///
    /// # Returns
    /// The value obtained by parsing [`crate::constants::DEFAULT_RETRY_DELAY`]
    /// using [`RetryDelay::from_str`].
    ///
    /// # Panics
    /// Panics if [`crate::constants::DEFAULT_RETRY_DELAY`] is not a valid
    /// [`RetryDelay`] string. That indicates a crate bug, not a caller mistake.
    #[inline]
    fn default() -> Self {
        Self::from_str(DEFAULT_RETRY_DELAY)
            .expect("DEFAULT_RETRY_DELAY must be a valid RetryDelay string")
    }
}
