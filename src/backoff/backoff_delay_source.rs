// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Stable source classification for a selected backoff delay.

/// Stable source classification for a selected delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffDelaySource {
    /// Delay selected solely from the configured policy.
    Policy,
    /// Delay explicitly selected by a retry rule.
    Explicit,
    /// Delay selected from a hint.
    Hint,
    /// Delay formed by merging a hint and policy delay.
    Merged,
}
