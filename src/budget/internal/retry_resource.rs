// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal resource labels for retry budgets.

/// Diagnostic labels retained by primitive budget values.
#[derive(Debug, Clone)]
pub enum RetryResource {
    /// The finite count of admitted attempts.
    Attempts,
    /// The finite sum of operation durations.
    OperationElapsed,
    /// The continuous whole-flow elapsed duration.
    TotalElapsed,
}
