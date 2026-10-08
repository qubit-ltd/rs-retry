// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Outcomes of checking whether a retry session may start an operation.

use std::num::NonZeroU32;

use qubit_clock::MonotonicInstant;

/// Outcome of checking admission for an externally scheduled session.
///
/// A waiting instant belongs to the clock domain supplied to the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum RetrySessionAdmission {
    /// An operation may start now with this one-based attempt number.
    Admitted(NonZeroU32),
    /// The same retry remains pending until this clock-domain instant.
    Waiting(MonotonicInstant),
}
