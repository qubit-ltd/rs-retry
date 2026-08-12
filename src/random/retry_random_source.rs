// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Random source abstraction for deterministic retry-delay selection.

/// Supplies random samples used by retry delay and jitter strategies.
///
/// Implementations must be safe for concurrent use because one retry policy
/// can be shared across threads. Every returned sample must lie within the
/// inclusive bounds supplied to the corresponding method.
pub trait RetryRandomSource: Send + Sync {
    /// Samples a floating-point value from an inclusive range.
    ///
    /// # Parameters
    ///
    /// * `min` - Inclusive finite lower bound.
    /// * `max` - Inclusive finite upper bound. Callers guarantee `min <= max`.
    ///
    /// # Returns
    ///
    /// A finite value in the inclusive range `min..=max`.
    fn random_f64_inclusive(&self, min: f64, max: f64) -> f64;
}
