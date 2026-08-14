// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Sources of attempt-level timeout failures.

use serde::Deserialize;
use serde::Serialize;

/// Timeout boundary that cancelled an admitted attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptTimeoutKind {
    /// The explicit timeout configured for one attempt.
    Attempt,
    /// The whole retry flow timeout fired while an attempt was running.
    Flow,
}
