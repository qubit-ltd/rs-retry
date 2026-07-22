// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Default retry random source backed by `rand::rng()`.

use rand::RngExt;

use crate::RetryRandomSource;

/// Default random source that samples from the current thread-local generator.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThreadRetryRandomSource;

impl RetryRandomSource for ThreadRetryRandomSource {
    /// Samples an integer from the requested inclusive range.
    ///
    /// # Parameters
    ///
    /// * `min` - Inclusive lower bound.
    /// * `max` - Inclusive upper bound.
    ///
    /// # Returns
    ///
    /// A uniformly distributed integer sample.
    #[inline]
    fn random_u64_inclusive(&self, min: u64, max: u64) -> u64 {
        rand::rng().random_range(min..=max)
    }

    /// Samples a floating-point value from the requested inclusive range.
    ///
    /// # Parameters
    ///
    /// * `min` - Inclusive lower bound.
    /// * `max` - Inclusive upper bound.
    ///
    /// # Returns
    ///
    /// A uniformly distributed floating-point sample.
    #[inline]
    fn random_f64_inclusive(&self, min: f64, max: f64) -> f64 {
        rand::rng().random_range(min..=max)
    }
}
