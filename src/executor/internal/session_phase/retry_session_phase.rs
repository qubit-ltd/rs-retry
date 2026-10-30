// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Legal phases of the single outstanding operation protocol.

use qubit_clock::MonotonicInstant;

/// Tracks the operation currently owned by an externally driven retry session.
///
/// The phase prevents admitting another operation while one is active and
/// retains the absolute deadline while the caller owns a pending retry wait.
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
