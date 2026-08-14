// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private base-delay strategy representation.

use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

/// Base-delay strategy used by [`super::super::BackoffPolicy`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// No delay.
    Immediate,
    /// A fixed delay.
    Fixed {
        /// Delay applied to each retry.
        delay: Duration,
    },
    /// A uniformly sampled delay range.
    Uniform {
        /// Inclusive lower bound.
        min: Duration,
        /// Inclusive upper bound.
        max: Duration,
    },
    /// A capped exponential delay.
    Exponential {
        /// Delay used for the first retry.
        initial: Duration,
        /// Multiplicative factor applied between retries.
        multiplier: f64,
        /// Maximum delay.
        max: Duration,
    },
}
