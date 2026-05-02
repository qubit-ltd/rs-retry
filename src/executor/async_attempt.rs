/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Type-erased asynchronous attempt used by the retry loop.

#[cfg(feature = "tokio")]
use super::async_attempt_future::AsyncAttemptFuture;

/// Type-erased asynchronous attempt used by the retry loop.
#[cfg(feature = "tokio")]
pub(in crate::executor) trait AsyncAttempt<E> {
    /// Calls the wrapped async operation once.
    ///
    /// # Returns
    /// A future resolving to `Ok(())` on success or an attempt failure.
    fn call(&mut self) -> AsyncAttemptFuture<'_, E>;
}
