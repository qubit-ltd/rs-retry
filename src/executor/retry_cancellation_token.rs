// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent cooperative cancellation for retry executions.

use std::sync::Arc;

use super::RetryCancelled;
use super::internal::RetryCancellationState;

/// A cloneable, runtime-independent cancellation token for retry executions.
///
/// Clones share cancellation state. A cancellation request is permanent and
/// wakes every future currently returned by
/// [`RetryCancellationToken::cancelled`].
#[derive(Clone, Debug, Default)]
pub struct RetryCancellationToken {
    /// State shared with cloned tokens and cancellation futures.
    pub(in crate::executor) state: Arc<RetryCancellationState>,
}

impl RetryCancellationToken {
    /// Creates a fresh non-cancelled token.
    ///
    /// # Returns
    /// A token whose cancellation flag is initially `false`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation and wakes all currently registered waiters.
    ///
    /// # Side Effects
    /// The first call permanently marks this token and all its clones as
    /// cancelled. Wakers are invoked after the internal registry lock has been
    /// released. Later calls have no effect.
    pub fn cancel(&self) {
        self.state.cancel();
    }

    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// `true` after this token or any of its clones has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Creates a future that completes when cancellation is requested.
    ///
    /// # Returns
    /// A future borrowing this token. Dropping a pending future unregisters its
    /// waker.
    #[must_use]
    pub fn cancelled(&self) -> RetryCancelled<'_> {
        RetryCancelled::new(self)
    }
}
