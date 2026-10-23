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
/// A session returns one of these values after the owner records an operation
/// result. A retry deadline remains in the session timer's clock domain; the
/// owner is responsible for waiting before admitting another attempt.
///
/// # Type Parameters
/// - `T`: Owned value produced by a successful operation.
/// - `E`: Owned application error supplied by a failed operation.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use qubit_clock::StdTimer;
/// use qubit_retry::{RetryConfig, RetrySession, RetrySessionAdmission, RetrySessionStep};
///
/// let config = RetryConfig::<&str>::builder().max_attempts(1).build()?;
/// let mut session = RetrySession::new(config, Arc::new(StdTimer::new()));
/// assert!(matches!(
///     session.begin_attempt().expect("fresh session admits its first attempt"),
///     RetrySessionAdmission::Admitted(_)
/// ));
/// let step = session.record_result::<u32>(Ok(7));
/// assert!(matches!(step, RetrySessionStep::Complete(success) if *success.value() == 7));
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
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
