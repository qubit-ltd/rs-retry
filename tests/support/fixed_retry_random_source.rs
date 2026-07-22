// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryRandomSource;

/// Deterministic random source that always returns configured samples.
pub(crate) struct FixedRetryRandomSource {
    /// Sample returned for integer ranges.
    u64_sample: u64,
    /// Sample returned for floating-point ranges.
    f64_sample: f64,
}

impl FixedRetryRandomSource {
    /// Creates a fixed random source.
    ///
    /// # Parameters
    ///
    /// * `u64_sample` - Sample returned for every integer range.
    /// * `f64_sample` - Sample returned for every floating-point range.
    ///
    /// # Returns
    ///
    /// A deterministic random source.
    #[inline(always)]
    pub(crate) const fn new(u64_sample: u64, f64_sample: f64) -> Self {
        Self {
            u64_sample,
            f64_sample,
        }
    }
}

impl RetryRandomSource for FixedRetryRandomSource {
    /// Returns the configured integer sample.
    ///
    /// # Parameters
    ///
    /// * `_min` - Inclusive lower bound ignored by this test source.
    /// * `_max` - Inclusive upper bound ignored by this test source.
    ///
    /// # Returns
    ///
    /// The configured integer sample.
    #[inline(always)]
    fn random_u64_inclusive(&self, _min: u64, _max: u64) -> u64 {
        self.u64_sample
    }

    /// Returns the configured floating-point sample.
    ///
    /// # Parameters
    ///
    /// * `_min` - Inclusive lower bound ignored by this test source.
    /// * `_max` - Inclusive upper bound ignored by this test source.
    ///
    /// # Returns
    ///
    /// The configured floating-point sample.
    #[inline(always)]
    fn random_f64_inclusive(&self, _min: f64, _max: f64) -> f64 {
        self.f64_sample
    }
}
