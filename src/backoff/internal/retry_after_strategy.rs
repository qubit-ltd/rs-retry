// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private retry-after hint resolution strategy.

use serde::Deserialize;
use serde::Serialize;

/// Resolution policy for caller-provided retry-after hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryAfterStrategy {
    /// Use the hint as the selected delay.
    PreferHint,
    /// Keep the larger of policy and hint delays.
    AtLeastBackoff,
    /// Discard the hint.
    IgnoreHint,
}
