// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry jitter applied on top of a base [`crate::RetryDelay`].
//!
//! After [`crate::RetryDelay`] yields a base sleep duration for the next
//! attempt, [`RetryJitter`] optionally perturbs it so concurrent retries do not
//! align on the same schedule.
//!
//! # Text interchange
//!
//! [`std::fmt::Display`] and [`std::str::FromStr`] use the same grammar:
//!
//! - `none` in any ASCII letter case (leading/trailing ASCII whitespace
//!   trimmed).
//! - `factor:` followed by a floating-point literal in **`[0.0, 1.0]`**;
//!   optional ASCII whitespace is allowed after the colon.
//!
//! The `factor:` prefix itself is **case-sensitive**. See
//! [`crate::constants::DEFAULT_RETRY_JITTER`] for the library default string.

use std::str::FromStr;
use std::time::Duration;

use parse_display::Display;
use qubit_argument::ArgumentResult;
use qubit_argument::require_that;
use serde::Deserialize;
use serde::Serialize;

use super::internal::RetryJitterFactorFormat;
use super::parse_retry_jitter_error::ParseRetryJitterError;
use crate::RetryDelay;
use crate::RetryRandomSource;
use crate::constants::DEFAULT_RETRY_JITTER;
use crate::error::argument_error_message;
use crate::random::ThreadRetryRandomSource;

/// Jitter strategy applied after a base [`crate::RetryDelay`] has been
/// calculated.
///
/// Supports [`RetryJitter::None`] and symmetric [`RetryJitter::Factor`] jitter.
/// After randomization, delays are clamped to **non-negative** values.
#[derive(Debug, Clone, Copy, PartialEq, Display, Serialize, Deserialize)]
pub enum RetryJitter {
    /// No jitter: [`RetryJitter::apply`] returns the base delay unchanged.
    #[display("none")]
    None,

    /// Symmetric relative jitter around the base delay.
    ///
    /// The inner `f64` is the relative half-span: jitter is drawn uniformly
    /// from `[-base * factor, base * factor]` nanoseconds (see
    /// [`RetryJitter::apply`]). It must be finite and lie in **`[0.0,
    /// 1.0]`** for validated configurations.
    #[display("factor:{0}")]
    Factor(
        /// Relative jitter half-span in the inclusive range `[0.0, 1.0]`.
        #[display(with = RetryJitterFactorFormat)]
        f64,
    ),
}

impl RetryJitter {
    /// Creates a no-jitter strategy.
    ///
    /// # Returns
    /// A [`RetryJitter::None`] strategy.
    #[inline(always)]
    pub fn none() -> Self {
        Self::None
    }

    /// Creates a symmetric relative jitter strategy.
    ///
    /// Validation requires `factor` to be finite and within `[0.0, 1.0]`.
    /// This constructor stores the value without validating it; call
    /// [`RetryJitter::validate`] for configuration or user input.
    ///
    /// # Arguments
    /// - `factor`: Relative jitter range. For example, `0.2` samples from `base
    ///   +/- 20%`.
    ///
    /// # Returns
    /// A [`RetryJitter::Factor`] strategy.
    #[inline(always)]
    pub fn factor(factor: f64) -> Self {
        Self::Factor(factor)
    }

    /// Applies jitter to a base delay.
    ///
    /// For [`RetryJitter::None`], returns `base` unchanged.
    ///
    /// For [`RetryJitter::Factor`], if `factor <= 0.0` or `base` is zero,
    /// returns `base` unchanged. Otherwise draws a uniform sample from the
    /// inclusive range `[-base * factor, base * factor]` in nanosecond
    /// space, adds it to the base, then clamps the result to **at least
    /// zero** (truncating the sum to `u64` nanoseconds). When `base`
    /// exceeds `u64::MAX` nanoseconds, this function returns `base`
    /// unchanged to avoid lossy downcasts.
    ///
    /// # Arguments
    /// - `base`: Base delay calculated by [`crate::RetryDelay`].
    ///
    /// # Returns
    /// The jittered delay, never below zero.
    #[inline(always)]
    pub fn apply(&self, base: Duration) -> Duration {
        self.apply_with_random_source(base, &ThreadRetryRandomSource)
    }

    /// Applies jitter with an explicit random source.
    ///
    /// # Parameters
    ///
    /// * `base` - Base delay calculated by [`crate::RetryDelay`].
    /// * `random_source` - Source used to sample the symmetric jitter span.
    ///
    /// # Returns
    ///
    /// The jittered delay, never below zero.
    pub fn apply_with_random_source(
        &self,
        base: Duration,
        random_source: &dyn RetryRandomSource,
    ) -> Duration {
        match self {
            Self::None => base,
            Self::Factor(factor)
                if !factor.is_finite() || *factor <= 0.0 || base.is_zero() =>
            {
                base
            }
            Self::Factor(factor) => {
                let base_nanos_u128 = base.as_nanos();
                if base_nanos_u128 > u64::MAX as u128 {
                    return base;
                }
                let base_nanos = base_nanos_u128 as f64;
                let span = base_nanos * factor;
                let jitter = random_source.random_f64_inclusive(-span, span);
                let nanos =
                    (base_nanos + jitter).clamp(0.0, u64::MAX as f64) as u64;
                Duration::from_nanos(nanos)
            }
        }
    }

    /// Calculates and jitters the delay for one retry attempt.
    ///
    /// This method combines base-delay strategy selection and jitter
    /// application into one step.
    ///
    /// # Arguments
    /// - `delay_strategy`: Base delay strategy used to calculate the attempt
    ///   delay.
    /// - `attempt`: Failed-attempt index passed to [`RetryDelay::base_delay`].
    ///
    /// # Returns
    /// The delay for the attempt after jitter is applied.
    #[inline(always)]
    pub fn delay_for_attempt(
        &self,
        delay_strategy: &RetryDelay,
        attempt: u32,
    ) -> Duration {
        self.delay_for_attempt_with_random_source(
            delay_strategy,
            attempt,
            &ThreadRetryRandomSource,
        )
    }

    /// Calculates and jitters one attempt delay with an explicit source.
    ///
    /// # Parameters
    ///
    /// * `delay_strategy` - Base delay strategy used for the attempt.
    /// * `attempt` - Failed-attempt index passed to
    ///   [`RetryDelay::base_delay_with_random_source`].
    /// * `random_source` - Source shared by base-delay and jitter sampling.
    ///
    /// # Returns
    ///
    /// The delay for the attempt after jitter is applied.
    #[inline(always)]
    pub fn delay_for_attempt_with_random_source(
        &self,
        delay_strategy: &RetryDelay,
        attempt: u32,
        random_source: &dyn RetryRandomSource,
    ) -> Duration {
        let base_delay = delay_strategy
            .base_delay_with_random_source(attempt, random_source);
        self.apply_with_random_source(base_delay, random_source)
    }

    /// Validates jitter parameters for use with executors and options.
    ///
    /// [`RetryJitter::None`] is always valid. For [`RetryJitter::Factor`], the
    /// factor must be finite and satisfy **`0.0 <= factor <= 1.0`**
    /// (endpoints included).
    ///
    /// # Returns
    /// `Ok(())` when the jitter configuration is usable.
    ///
    /// # Errors
    /// Returns an error when the factor is negative, greater than `1.0`, NaN,
    /// or infinite.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_argument("jitter_factor")
            .map_err(argument_error_message)
    }

    /// Validates jitter with structured argument error context.
    ///
    /// # Arguments
    /// - `path`: Configuration path associated with the jitter factor.
    ///
    /// # Returns
    /// `Ok(())` when the jitter strategy is usable.
    ///
    /// # Errors
    /// Returns an argument error at `path` when the factor is non-finite or
    /// outside the inclusive range from zero to one.
    pub(super) fn validate_argument(&self, path: &str) -> ArgumentResult<()> {
        match self {
            Self::None => Ok(()),
            Self::Factor(factor) => {
                require_that(
                    *factor,
                    path,
                    |factor| factor.is_finite() && (0.0..=1.0).contains(factor),
                    "jitter_factor_range",
                    "jitter factor must be finite and in range [0.0, 1.0]",
                )?;
                Ok(())
            }
        }
    }
}

impl FromStr for RetryJitter {
    type Err = ParseRetryJitterError;

    /// Parses the canonical retry-jitter text representation.
    ///
    /// # Parameters
    ///
    /// * `input` - Text containing `none` or `factor:<f64>`.
    ///
    /// # Returns
    ///
    /// The parsed jitter strategy.
    ///
    /// # Errors
    ///
    /// Returns [`ParseRetryJitterError`] when the format is unsupported, the
    /// factor is not numeric, or the factor is non-finite or outside
    /// `[0.0, 1.0]`.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.eq_ignore_ascii_case("none") {
            return Ok(Self::None);
        }
        let factor = input
            .strip_prefix("factor:")
            .ok_or_else(ParseRetryJitterError::invalid_format)?
            .trim()
            .parse::<f64>()
            .map_err(|_| ParseRetryJitterError::invalid_factor())?;
        if !factor.is_finite() || !(0.0..=1.0).contains(&factor) {
            return Err(ParseRetryJitterError::factor_out_of_range());
        }
        Ok(Self::Factor(factor))
    }
}

impl Default for RetryJitter {
    /// Creates the default jitter strategy.
    ///
    /// # Returns
    /// The value obtained by parsing [`crate::constants::DEFAULT_RETRY_JITTER`]
    /// using [`RetryJitter::from_str`].
    ///
    /// # Panics
    /// Panics if [`crate::constants::DEFAULT_RETRY_JITTER`] is not a valid
    /// [`RetryJitter`] string. That indicates a crate bug, not a caller
    /// mistake.
    #[inline]
    fn default() -> Self {
        Self::from_str(DEFAULT_RETRY_JITTER)
            .expect("DEFAULT_RETRY_JITTER must be a valid RetryJitter string")
    }
}
