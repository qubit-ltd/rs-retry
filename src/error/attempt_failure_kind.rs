// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable semantic classification for attempt failures.

use serde::Deserialize;
use serde::Serialize;

/// Semantic kind of a single attempt failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptFailureKind {
    /// The operation returned its application error.
    Error,
    /// The attempt exceeded its effective timeout.
    Timeout,
    /// The isolated attempt panicked.
    Panic,
    /// The retry executor failed to run the attempt normally.
    Executor,
}
