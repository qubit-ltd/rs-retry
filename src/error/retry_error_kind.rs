// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable classification for terminal retry errors.

use serde::Deserialize;
use serde::Serialize;

/// Stable category of a terminal retry error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryErrorKind {
    /// A caller or rule deliberately aborted the flow.
    Aborted,
    /// A retry continuation budget was exhausted.
    Exhausted,
    /// An execution timeout stopped the flow.
    TimedOut,
    /// Retry infrastructure could not continue safely.
    Infrastructure,
}
