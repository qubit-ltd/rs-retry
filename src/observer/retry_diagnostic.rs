// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Diagnostics produced while invoking retry callbacks.

use super::retry_diagnostic_kind::RetryDiagnosticKind;

/// Structured callback diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryDiagnostic {
    kind: RetryDiagnosticKind,
    callback_index: usize,
}

impl RetryDiagnostic {
    /// Creates a callback diagnostic.
    #[allow(dead_code)]
    pub(crate) fn new(
        kind: RetryDiagnosticKind,
        callback_index: usize,
    ) -> Self {
        Self {
            kind,
            callback_index,
        }
    }

    /// Returns the callback category.
    pub fn kind(&self) -> RetryDiagnosticKind {
        self.kind
    }

    /// Returns the callback index in registration order.
    pub fn callback_index(&self) -> usize {
        self.callback_index
    }
}
