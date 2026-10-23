// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! State phases for an externally driven retry session.

use qubit_clock::MonotonicInstant;

/// Legal phases of the single outstanding operation protocol.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::executor) enum RetrySessionPhase {
    /// No attempt has started.
    Ready,
    /// One admitted operation awaits its result.
    Active,
    /// An external scheduler owns the pending retry wait.
    Waiting(MonotonicInstant),
    /// Completion observers have been notified.
    Finished,
}
