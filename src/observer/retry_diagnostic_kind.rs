// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable retry callback diagnostic categories.

/// Callback category that panicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDiagnosticKind {
    /// A retry rule panicked.
    RulePanicked,
    /// An observer panicked.
    ObserverPanicked,
}
