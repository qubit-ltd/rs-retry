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

/// Outcome of checking whether an externally scheduled retry session may
/// start an operation.
///
/// The caller is responsible for scheduling another admission check after a
/// retry deadline; this type never waits or registers a timer. A waiting
/// instant belongs to the monotonic clock domain supplied to the session, so
/// it must only be compared or scheduled using that same clock domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum RetrySessionAdmission {
    /// An operation may start now with this one-based attempt number.
    Admitted(NonZeroU32),
    /// The same retry remains pending until this instant in the session's
    /// monotonic clock domain. The caller should wait until then and call
    /// `begin_attempt` again; an early check may return this outcome again.
    Waiting(MonotonicInstant),
}
