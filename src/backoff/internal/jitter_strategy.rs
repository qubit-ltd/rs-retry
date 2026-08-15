// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private jitter strategy representation.

/// Jitter applied to a policy or hint delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JitterStrategy {
    /// Do not vary the selected delay.
    None,
    /// Sample from zero through the selected delay.
    Full,
    /// Apply a symmetric multiplicative range.
    Bounded {
        /// Maximum relative deviation.
        ratio: f64,
    },
}
