/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry jitter applied on top of a base [`crate::RetryDelay`].
//!
//! After [`crate::RetryDelay`] yields a base sleep duration for the next attempt,
//! [`RetryJitter`] optionally perturbs it so concurrent retries do not align on the
//! same schedule.
//!
//! # Text interchange
//!
//! [`std::fmt::Display`] and [`std::str::FromStr`] use the same grammar:
//!
//! - `none` in any ASCII letter case (leading/trailing ASCII whitespace trimmed).
//! - `factor:` followed by a floating-point literal in **`[0.0, 1.0]`**; optional
//!   ASCII whitespace is allowed after the colon.
//!
//! The `factor:` prefix itself is **case-sensitive**. See
//! [`crate::constants::DEFAULT_RETRY_JITTER`] for the library default string.
//!
//! Author: Haixing Hu

use std::str::FromStr;
use std::time::Duration;

use parse_display::{Display, DisplayFormat, FromStr as DeriveFromStr, FromStrFormat, ParseError};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::RetryDelay;
use crate::constants::DEFAULT_RETRY_JITTER;

/// Jitter strategy applied after a base [`crate::RetryDelay`] has been calculated.
///
/// Supports [`RetryJitter::None`] and symmetric [`RetryJitter::Factor`] jitter.
/// After randomization, delays are clamped to **non-negative** values.
#[derive(Debug, Clone, Copy, PartialEq, Display, DeriveFromStr, Serialize, Deserialize)]
pub enum RetryJitter {
    /// No jitter: [`RetryJitter::apply`] returns the base delay unchanged.
    #[display("none")]
    #[from_str(regex = r"(?i)\s*none\s*")]
    None,

    /// Symmetric relative jitter around the base delay.
    ///
    /// The inner `f64` is the relative half-span: jitter is drawn uniformly from
    /// `[-base * factor, base * factor]` nanoseconds (see [`RetryJitter::apply`]).
    /// It must be finite and lie in **`[0.0, 1.0]`** for validated configurations.
    #[display("factor:{0}")]
    #[from_str(regex = r"\s*factor:\s*(?<0>\S(?:.*\S)?)\s*")]
    Factor(#[display(with = RetryJitterFactorFormat)] f64),
}

/// Formats jitter factors as `f64` text and parses with range validation.
struct RetryJitterFactorFormat;

impl DisplayFormat<f64> for RetryJitterFactorFormat {
    /// Writes the factor using the default `f64` formatter.
    ///
    /// # Parameters
    /// - `f`: Output formatter.
    /// - `value`: Factor value.
    ///
    /// # Returns
    /// `Ok(())` on success, or [`std::fmt::Error`] if formatting fails.
    ///
    /// # Errors
    /// Returns [`std::fmt::Error`] only if the formatter rejects output.
    fn write(&self, f: &mut std::fmt::Formatter<'_>, value: &f64) -> std::fmt::Result {
        write!(f, "{value}")
    }
}

impl FromStrFormat<f64> for RetryJitterFactorFormat {
    /// Error returned by factor parsing.
    type Err = ParseError;

    /// Parses and validates a factor in range `[0.0, 1.0]`.
    ///
    /// # Parameters
    /// - `s`: Raw factor text captured by `parse-display`.
    ///
    /// # Returns
    /// The parsed factor.
    ///
    /// # Errors
    /// Returns [`ParseError`] when the input is not a valid `f64` or lies outside
    /// `[0.0, 1.0]`, including non-finite values.
    fn parse(&self, s: &str) -> Result<f64, Self::Err> {
        let value = s
            .parse::<f64>()
            .map_err(|_| ParseError::with_message("invalid retry jitter factor"))?;
        if !(0.0..=1.0).contains(&value) {
            return Err(ParseError::with_message(
                "retry jitter factor must be in range [0.0, 1.0]",
            ));
        }
        Ok(value)
    }
}

impl RetryJitter {
    /// Creates a no-jitter strategy.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A [`RetryJitter::None`] strategy.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    pub fn none() -> Self {
        Self::None
    }

    /// Creates a symmetric relative jitter strategy.
    ///
    /// Validation requires `factor` to be finite and within `[0.0, 1.0]`.
    ///
    /// # Parameters
    /// - `factor`: Relative jitter range. For example, `0.2` samples from
    ///   `base +/- 20%`.
    ///
    /// # Returns
    /// A [`RetryJitter::Factor`] strategy.
    ///
    /// # Errors
    /// This constructor does not validate `factor`; use [`RetryJitter::validate`]
    /// before applying values that come from configuration or user input.
    #[inline]
    pub fn factor(factor: f64) -> Self {
        Self::Factor(factor)
    }

    /// Applies jitter to a base delay.
    ///
    /// For [`RetryJitter::None`], returns `base` unchanged.
    ///
    /// For [`RetryJitter::Factor`], if `factor <= 0.0` or `base` is zero, returns
    /// `base` unchanged. Otherwise draws a uniform sample from the inclusive range
    /// `[-base * factor, base * factor]` in nanosecond space, adds it to the base,
    /// then clamps the result to **at least zero** (truncating the sum to `u64`
    /// nanoseconds). When `base` exceeds `u64::MAX` nanoseconds, this function
    /// returns `base` unchanged to avoid lossy downcasts.
    ///
    /// # Parameters
    /// - `base`: Base delay calculated by [`crate::RetryDelay`].
    ///
    /// # Returns
    /// The jittered delay, never below zero.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic for non-finite factors. Non-finite values
    /// gracefully fall back to returning `base`.
    pub fn apply(&self, base: Duration) -> Duration {
        match self {
            Self::None => base,
            Self::Factor(factor) if !factor.is_finite() || *factor <= 0.0 || base.is_zero() => base,
            Self::Factor(factor) => {
                let base_nanos_u128 = base.as_nanos();
                if base_nanos_u128 > u64::MAX as u128 {
                    return base;
                }
                let base_nanos = base_nanos_u128 as f64;
                let span = base_nanos * factor;
                let mut rng = rand::rng();
                let jitter = rng.random_range(-span..=span);
                let nanos = (base_nanos + jitter).clamp(0.0, u64::MAX as f64) as u64;
                Duration::from_nanos(nanos)
            }
        }
    }

    /// Calculates and jitters the delay for one retry attempt.
    ///
    /// This method combines base-delay strategy selection and jitter application
    /// into one step.
    ///
    /// # Parameters
    /// - `delay_strategy`: Base delay strategy used to calculate the attempt
    ///   delay.
    /// - `attempt`: Failed-attempt index passed to
    ///   [`RetryDelay::base_delay`].
    ///
    /// # Returns
    /// The delay for the attempt after jitter is applied.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic for non-finite factors. Non-finite values
    /// gracefully fall back to returning the base delay.
    pub fn delay_for_attempt(&self, delay_strategy: &RetryDelay, attempt: u32) -> Duration {
        let base_delay = delay_strategy.base_delay(attempt);
        self.apply(base_delay)
    }

    /// Validates jitter parameters for use with executors and options.
    ///
    /// [`RetryJitter::None`] is always valid. For [`RetryJitter::Factor`], the factor
    /// must be finite and satisfy **`0.0 <= factor <= 1.0`** (endpoints included).
    ///
    /// # Returns
    /// `Ok(())` when the jitter configuration is usable.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Errors
    /// Returns an error when the factor is negative, greater than `1.0`, NaN,
    /// or infinite.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::None => Ok(()),
            Self::Factor(factor) => {
                if !factor.is_finite() || *factor < 0.0 || *factor > 1.0 {
                    Err("jitter factor must be finite and in range [0.0, 1.0]".to_string())
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Default for RetryJitter {
    /// Creates the default jitter strategy.
    ///
    /// # Returns
    /// The value obtained by parsing [`crate::constants::DEFAULT_RETRY_JITTER`]
    /// using [`RetryJitter::from_str`].
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// Panics if [`crate::constants::DEFAULT_RETRY_JITTER`] is not a valid
    /// [`RetryJitter`] string. That indicates a crate bug, not a caller mistake.
    #[inline]
    fn default() -> Self {
        Self::from_str(DEFAULT_RETRY_JITTER)
            .expect("DEFAULT_RETRY_JITTER must be a valid RetryJitter string")
    }
}
