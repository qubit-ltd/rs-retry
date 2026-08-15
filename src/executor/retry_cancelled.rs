// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Future returned by a retry cancellation token.

use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use super::RetryCancellationToken;

/// Future returned by [`RetryCancellationToken::cancelled`].
///
/// The future is runtime-independent and keeps at most one registered waker.
#[derive(Debug)]
pub struct RetryCancelled<'a> {
    /// Token whose cancellation state this future observes.
    token: &'a RetryCancellationToken,
    /// Stable registry entry allocated on the first pending poll.
    registration_id: Option<u64>,
}

impl<'a> RetryCancelled<'a> {
    /// Creates a future observing cancellation of `token`.
    pub(super) fn new(token: &'a RetryCancellationToken) -> Self {
        Self {
            token,
            registration_id: None,
        }
    }
}

impl Future for RetryCancelled<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.token.is_cancelled() {
            return Poll::Ready(());
        }
        let waker = context.waker().clone();
        let (registration_id, replaced_waker, removed_waker, cancelled) =
            this.token.state.register(this.registration_id, waker);
        this.registration_id = Some(registration_id);
        if cancelled {
            this.registration_id = None;
            drop(replaced_waker);
            drop(removed_waker);
            Poll::Ready(())
        } else {
            drop(replaced_waker);
            Poll::Pending
        }
    }
}

impl Drop for RetryCancelled<'_> {
    fn drop(&mut self) {
        if let Some(registration_id) = self.registration_id.take() {
            let removed_waker = self.token.state.unregister(registration_id);
            drop(removed_waker);
        }
    }
}
