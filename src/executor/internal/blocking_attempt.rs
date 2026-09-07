// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Type-erased blocking worker attempt used by the retry loop.

use crate::AttemptFailure;
use crate::executor::AttemptCancellationToken;

/// Type-erased blocking worker attempt used by the retry loop.
/// # Type Parameters
/// - `E`: Application error transferred through the worker result channel.
pub(in crate::executor) trait BlockingAttempt<E>: Send + Sync {
    /// Calls the wrapped operation once.
    ///
    /// # Parameters
    /// - `token`: Cooperative cancellation token for this attempt.
    ///
    /// # Returns
    /// `Ok(())` when the operation succeeded, or an attempt failure otherwise.
    ///
    /// # Errors
    /// Returns the original application failure without cloning it.
    ///
    /// # Panics
    /// The operation may unwind; the worker executor owns the panic capture
    /// boundary.
    fn call(&self, token: AttemptCancellationToken) -> Result<(), AttemptFailure<E>>;
}
