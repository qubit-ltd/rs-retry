/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Jitter applied to retry delays.
//!
//! Jitter is applied after the base [`crate::Delay`] has been calculated. It
//! helps callers avoid retry bursts when multiple tasks fail at the same time.

use std::time::Duration;

use rand::RngExt;

/// Jitter applied after a base [`crate::Delay`] has been calculated.
///
/// The current implementation supports no jitter and symmetric factor-based
/// jitter. Factor jitter keeps the lower bound at zero to avoid negative
/// durations after randomization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Jitter {
    /// No jitter.
    None,

    /// Symmetric relative jitter: `base +/- base * factor`.
    Factor(f64),
}

impl Jitter {
    /// Creates a no-jitter strategy.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A [`Jitter::None`] strategy.
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
    /// A [`Jitter::Factor`] strategy.
    ///
    /// # Errors
    /// This constructor does not validate `factor`; use [`Jitter::validate`]
    /// before applying values that come from configuration or user input.
    #[inline]
    pub fn factor(factor: f64) -> Self {
        Self::Factor(factor)
    }

    /// Applies jitter to a base delay.
    ///
    /// A zero base delay is returned unchanged. Factor jitter samples a value
    /// from the inclusive range `[-base * factor, base * factor]`.
    ///
    /// # Parameters
    /// - `base`: Base delay calculated by [`crate::Delay`].
    ///
    /// # Returns
    /// The jittered delay, never below zero.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// May panic if a [`Jitter::Factor`] value has not been validated and the
    /// factor is non-finite, because the random range cannot be sampled.
    pub fn apply(&self, base: Duration) -> Duration {
        match self {
            Self::None => base,
            Self::Factor(factor) if *factor <= 0.0 || base.is_zero() => base,
            Self::Factor(factor) => {
                let base_nanos = base.as_nanos() as f64;
                let span = base_nanos * factor;
                let mut rng = rand::rng();
                let jitter = rng.random_range(-span..=span);
                Duration::from_nanos((base_nanos + jitter).max(0.0) as u64)
            }
        }
    }

    /// Validates jitter parameters.
    ///
    /// Returns a human-readable message when the factor is negative, greater
    /// than `1.0`, NaN, or infinite.
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

impl Default for Jitter {
    /// Creates the default jitter strategy.
    ///
    /// # Returns
    /// [`Jitter::None`].
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    fn default() -> Self {
        Self::None
    }
}
