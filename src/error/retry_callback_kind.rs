// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry callback classifications.

use std::fmt;

/// Callback category that failed while controlling a retry flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryCallbackKind {
    /// A retry rule callback.
    Rule,
    /// A retry observer callback.
    Observer,
}

impl fmt::Display for RetryCallbackKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Rule => "rule",
            Self::Observer => "observer",
        };
        formatter.write_str(name)
    }
}
