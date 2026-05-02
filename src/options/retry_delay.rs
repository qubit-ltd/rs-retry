/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! RetryDelay strategies for retry attempts.
//!
//! A [`RetryDelay`] produces the base sleep duration after a failed attempt. The
//! base duration is calculated before [`crate::RetryJitter`] is applied by a retry
//! executor.
//!
//! # Text interchange
//!
//! [`std::fmt::Display`] and [`std::str::FromStr`] share a canonical string form:
//!
//! - `none`
//! - `fixed(<duration>)` — duration fields are displayed as saturated whole milliseconds
//!   with an `ms` suffix; `FromStr` accepts any duration string parsed by
//!   [`qubit_serde::serde::duration_with_unit`]
//! - `random(<min>..=<max>)` — same rules for the two duration fields
//! - `exponential(initial=<...>, max=<...>, multiplier=<f64>)` — same for `initial` and `max`
//!
//! For [`std::str::FromStr`], substrings for duration fields follow
//! [`qubit_serde::serde::duration_with_unit`] (bare integer as milliseconds, unit
//! suffixes, etc.; see that module). [`std::fmt::Display`] normalizes to whole
//! millisecond + `ms` for those fields.

use std::str::FromStr;
use std::time::Duration;

use parse_display::{Display, FromStr};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::retry_delay_duration_format::RetryDelayDurationFormat;
use crate::constants::DEFAULT_RETRY_DELAY;

/// Base delay strategy before jitter is applied.
///
/// RetryDelay strategies are value types that can be reused across executors. Random
/// and exponential strategies are validated separately by [`RetryDelay::validate`],
/// which is called when building [`crate::RetryOptions`].
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
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A [`RetryDelay::None`] strategy.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    pub fn none() -> Self {
        Self::None
    }

    /// Creates a fixed-delay strategy.
    ///
    /// # Parameters
    /// - `delay`: Duration slept after each failed attempt.
    ///
    /// # Returns
    /// A [`RetryDelay::Fixed`] strategy.
    ///
    /// # Errors
    /// This constructor does not validate `delay`; use [`RetryDelay::validate`] to
    /// reject a zero duration.
    #[inline]
    pub fn fixed(delay: Duration) -> Self {
        Self::Fixed(delay)
    }

    /// Creates a random-delay strategy.
    ///
    /// # Parameters
    /// - `min`: Inclusive lower bound for generated delays.
    /// - `max`: Inclusive upper bound for generated delays.
    ///
    /// # Returns
    /// A [`RetryDelay::Random`] strategy.
    ///
    /// # Errors
    /// This constructor does not validate the range; use [`RetryDelay::validate`] to
    /// reject a zero minimum or a minimum greater than the maximum.
    #[inline]
    pub fn random(min: Duration, max: Duration) -> Self {
        Self::Random { min, max }
    }

    /// Creates an exponential-backoff strategy.
    ///
    /// # Parameters
    /// - `initial`: RetryDelay used for the first retry.
    /// - `max`: Upper bound applied to every calculated delay.
    /// - `multiplier`: Factor applied for each subsequent failed attempt.
    ///
    /// # Returns
    /// A [`RetryDelay::Exponential`] strategy.
    ///
    /// # Errors
    /// This constructor does not validate the parameters; use
    /// [`RetryDelay::validate`] to reject a zero initial delay, `max < initial`, or
    /// a multiplier that is non-finite or less than or equal to `1.0`.
    #[inline]
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
    ///
    /// # Parameters
    /// - `attempt`: Failed attempt number. Values `0` and `1` are treated as
    ///   the first exponential-backoff step.
    ///
    /// # Returns
    /// The base delay before jitter is applied.
    ///
    /// # Errors
    /// This function does not return errors. Invalid strategies should be
    /// rejected with [`RetryDelay::validate`] before they are used in an executor.
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

    /// Converts a [`Duration`] to whole nanoseconds as `u64`.
    ///
    /// Values larger than [`u64::MAX`] nanoseconds are saturated to
    /// [`u64::MAX`] so the result fits in `u64` for uniform random delay sampling
    /// in [`RetryDelay::base_delay`].
    ///
    /// # Parameters
    /// - `duration`: Duration to convert.
    ///
    /// # Returns
    /// The duration in nanoseconds, capped at [`u64::MAX`].
    ///
    /// # Errors
    /// This function does not return errors.
    fn duration_to_nanos_u64(duration: Duration) -> u64 {
        duration.as_nanos().min(u64::MAX as u128) as u64
    }

    /// Computes the exponential backoff delay for a given failed-attempt index.
    ///
    /// The effective exponent is `attempt.saturating_sub(1)`, so attempts `0`
    /// and `1` both yield the initial delay (matching [`RetryDelay::base_delay`]).
    /// Each further attempt multiplies the base nanosecond count by
    /// `multiplier` that many times, then the result is capped at `max`.
    ///
    /// # Parameters
    /// - `initial`: RetryDelay for the first retry step (attempts `0` and `1`).
    /// - `max`: Upper bound on the returned delay.
    /// - `multiplier`: Factor applied per additional attempt beyond the first.
    /// - `attempt`: Failed attempt number (see [`RetryDelay::base_delay`]).
    ///
    /// # Returns
    /// The computed delay, or `max` when the scaled value is not finite or is
    /// not less than `max` in nanoseconds.
    ///
    /// # Errors
    /// This function does not return errors. Callers must ensure parameters
    /// satisfy [`RetryDelay::validate`] when constructing a public executor.
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

    /// Validates strategy parameters.
    ///
    /// Returns a human-readable message describing the invalid field when the
    /// strategy cannot be used safely by an executor.
    ///
    /// # Returns
    /// `Ok(())` when all parameters are usable; otherwise an error message that
    /// can be wrapped by [`crate::RetryConfigError`].
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Errors
    /// Returns an error when a fixed delay is zero, a random range is invalid,
    /// or exponential backoff parameters are zero, inverted, non-finite, or too
    /// small.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::None => Ok(()),
            Self::Fixed(delay) => {
                if delay.is_zero() {
                    Err("fixed delay cannot be zero".to_string())
                } else {
                    Ok(())
                }
            }
            Self::Random { min, max } => {
                if min.is_zero() {
                    Err("random delay minimum cannot be zero".to_string())
                } else if min > max {
                    Err("random delay minimum cannot be greater than maximum".to_string())
                } else {
                    Ok(())
                }
            }
            Self::Exponential {
                initial,
                max,
                multiplier,
            } => {
                if initial.is_zero() {
                    Err("exponential delay initial value cannot be zero".to_string())
                } else if max < initial {
                    Err("exponential delay maximum cannot be smaller than initial".to_string())
                } else if !multiplier.is_finite() || *multiplier <= 1.0 {
                    Err(
                        "exponential delay multiplier must be finite and greater than 1.0"
                            .to_string(),
                    )
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Default for RetryDelay {
    /// Creates the default exponential-backoff strategy.
    ///
    /// # Returns
    /// The value obtained by parsing [`crate::constants::DEFAULT_RETRY_DELAY`]
    /// using [`RetryDelay::from_str`].
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Errors
    /// This function does not return errors.
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
