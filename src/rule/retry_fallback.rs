// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Fallback behavior for unclassified retry failures.

/// Default action when every retry rule delegates an application error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetryFallback {
    /// Stop after the first unclassified application error.
    #[default]
    Abort,
    /// Retry unclassified application errors using the policy backoff.
    Retry,
}
