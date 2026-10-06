// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Outcome of recording one operation in an owned retry session.

use qubit_clock::MonotonicInstant;

use crate::RetryError;
use crate::RetrySuccess;

/// Result of recording one operation in a [`super::RetrySession`].
///
/// `T` is the successful value and `E` is the owned application error.
#[derive(Debug)]
#[must_use]
pub enum RetrySessionStep<T, E> {
    /// The operation succeeded; completion observers have been notified.
    Complete(RetrySuccess<T>),
    /// Resume at this absolute instant in the session timer's clock domain.
    /// The owner supplies the wait and must call `begin_attempt` again.
    RetryAt(MonotonicInstant),
    /// The flow terminated; completion observers have been notified.
    Failed(RetryError<E>),
}
