// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Presence of the optional jitter ratio on the wire.

/// Presence-aware jitter ratio field used to distinguish absent from `null`.
#[derive(Clone, Copy, Default)]
pub(super) enum RatioField {
    /// The ratio field was absent.
    #[default]
    Missing,
    /// The ratio field was present with a numeric value.
    Present(
        /// Numeric ratio, validated by the enclosing backoff policy.
        f64,
    ),
}
