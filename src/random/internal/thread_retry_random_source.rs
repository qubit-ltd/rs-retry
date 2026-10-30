// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Default retry random source backed by `fastrand`'s thread-local generator.

use fastrand::f64_inclusive;

use crate::RetryRandomSource;

/// Default random source that samples from the current thread-local generator.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThreadRetryRandomSource;

impl RetryRandomSource for ThreadRetryRandomSource {
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
    ///
    /// # Panics
    /// In debug builds, asserts that the bounds are finite and ordered. Retry
    /// policies request samples from the validated range `0.0..=1.0`.
    #[inline]
    fn random_f64_inclusive(&self, min: f64, max: f64) -> f64 {
        debug_assert!(min.is_finite() && max.is_finite() && min <= max);
        if min == max {
            return min;
        }
        min + (max - min) * f64_inclusive()
    }
}
