// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cooperative cancellation token for blocking attempts.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

/// Cooperative cancellation token passed to blocking timeout attempts.
///
/// During [`WorkerRetry::run`](crate::WorkerRetry::run), the executor marks
/// this token as cancelled when it requests the current worker to stop, such
/// as after a timeout or flow cancellation. The worker must check
/// [`AttemptCancellationToken::is_cancelled`] and return on its own; the
/// executor cannot safely kill Rust threads.
///
/// # Examples
///
/// ```
/// use qubit_retry::AttemptCancellationToken;
///
/// let token = AttemptCancellationToken::new();
/// let worker_token = token.clone();
/// token.cancel();
/// assert!(worker_token.is_cancelled());
/// ```
#[derive(Debug, Clone, Default)]
pub struct AttemptCancellationToken {
    /// Cancellation state shared by all clones of this token.
    cancelled: Arc<AtomicBool>,
}

impl AttemptCancellationToken {
    /// Creates a fresh non-cancelled token.
    ///
    /// # Returns
    /// A token whose cancellation flag is initially `false`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// `true` after the executor or another holder calls
    /// [`AttemptCancellationToken::cancel`].
    #[inline]
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Marks this token as cancelled.
    ///
    /// # Side Effects
    /// Sets the shared cancellation flag. Clones of this token observe the same
    /// flag.
    #[inline]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}
