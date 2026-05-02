/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Cooperative cancellation token for blocking attempts.
//!

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative cancellation token passed to blocking timeout attempts.
///
/// The retry executor marks the token as cancelled when
/// [`crate::Retry::run_in_worker`] stops waiting for a timed-out worker thread.
/// The worker must check [`AttemptCancelToken::is_cancelled`] and return on its
/// own; Rust threads cannot be safely killed by the executor.
#[derive(Debug, Clone, Default)]
pub struct AttemptCancelToken {
    /// Shared cancellation flag.
    cancelled: Arc<AtomicBool>,
}

impl AttemptCancelToken {
    /// Creates a fresh non-cancelled token.
    ///
    /// # Returns
    /// A token whose cancellation flag is initially `false`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks this token as cancelled.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Side Effects
    /// Sets the shared cancellation flag. Clones of this token observe the same
    /// flag.
    #[inline]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// `true` after the executor or another holder calls
    /// [`AttemptCancelToken::cancel`].
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
