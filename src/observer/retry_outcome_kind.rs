// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Terminal outcome category observed by retry observers.

/// Stable terminal outcome category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcomeKind {
    /// An attempt succeeded.
    Succeeded,
    /// The retry flow stopped with an error.
    Failed,
}
