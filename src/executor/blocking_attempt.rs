/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Type-erased blocking worker attempt used by the retry loop.

use super::attempt_cancel_token::AttemptCancelToken;
use crate::AttemptFailure;

/// Type-erased blocking worker attempt used by the retry loop.
pub(in crate::executor) trait BlockingAttempt<E>: Send + Sync {
    /// Calls the wrapped operation once.
    ///
    /// # Parameters
    /// - `token`: Cooperative cancellation token for this attempt.
    ///
    /// # Returns
    /// `Ok(())` when the operation succeeded, or an attempt failure otherwise.
    fn call(&self, token: AttemptCancelToken) -> Result<(), AttemptFailure<E>>;
}
