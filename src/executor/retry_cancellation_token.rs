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
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryCancellationPhase;
/// use qubit_retry::RetryCancellationToken;
/// use qubit_retry::RetryErrorReason;
/// use qubit_retry::RetryPolicy;
///
/// fn main() {
///     let token = RetryCancellationToken::new();
///     let retry = Retry::<&str>::builder(RetryPolicy::builder().build().unwrap()).build();
///     let error = retry.sync().cancellation_token(token.clone()).run(|| {
///         token.cancel();
///         Err::<(), _>("temporary read failure")
///     }).unwrap_err();
///     assert_eq!(error.context().attempts(), 1);
///     assert!(matches!(error.reason(), RetryErrorReason::Cancelled {
///         phase: RetryCancellationPhase::Backoff, ..
///     }));
/// }
/// ```
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
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a future that completes when cancellation is requested.
    ///
    /// # Returns
    /// A future borrowing this token. Dropping a pending future unregisters its
    /// waker.
    #[must_use]
    #[inline(always)]
    pub fn cancelled(&self) -> RetryCancelled<'_> {
        RetryCancelled::new(self)
    }
    /// Returns whether this token and `other` share one cancellation source.
    ///
    /// Tokens cloned from one another share a source, so cancellation requested
    /// through either token is observed by both. Independently created tokens
    /// do not share a source, even when they currently have the same cancelled
    /// state.
    ///
    /// # Parameters
    ///
    /// - `other`: The token whose cancellation source is compared with this
    ///   token's source.
    ///
    /// # Returns
    ///
    /// `true` when both tokens refer to the same cancellation source; `false`
    /// otherwise.
    #[inline(always)]
    #[must_use]
    pub fn shares_source_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// `true` after this token or any of its clones has been cancelled.
    #[must_use]
    #[inline(always)]
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Requests cancellation and wakes all currently registered waiters.
    ///
    /// # Side Effects
    /// The first call permanently marks this token and all its clones as
    /// cancelled. Wakers are invoked after the internal registry lock has been
    /// released. Later calls have no effect.
    #[inline(always)]
    pub fn cancel(&self) {
        self.state.cancel();
    }
}
