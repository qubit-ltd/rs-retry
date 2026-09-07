// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Strict deserialization discriminant for jitter strategies.

use serde::Deserialize;

/// Deserialization-only jitter tag used to reject irrelevant variant fields.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum JitterStrategyTag {
    /// Do not vary the selected delay.
    None,
    /// Sample from zero through the selected delay.
    Full,
    /// Apply a symmetric multiplicative range.
    Bounded,
}
