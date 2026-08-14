// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent cooperative cancellation for retry executions.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

/// Identifies one future's stable entry in the waker registry.
type RegistrationId = u64;

/// Shared state for a [`RetryCancellationToken`].
#[derive(Debug, Default)]
struct Inner {
    /// Whether cancellation has been requested.
    cancelled: AtomicBool,

    /// Wakers waiting for the first cancellation request.
    waiters: Mutex<WakerRegistry>,
}

impl Inner {
    /// Locks the waker registry, recovering its contents after poisoning.
    fn lock_waiters(&self) -> MutexGuard<'_, WakerRegistry> {
        self.waiters
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Tracks the current waker for each pending cancellation future.
#[derive(Debug, Default)]
struct WakerRegistry {
    /// Identifier to consider for the next new registration.
    next_id: RegistrationId,

    /// Current wakers keyed by their future's stable identifier.
    wakers: HashMap<RegistrationId, Waker>,
}

impl WakerRegistry {
    /// Registers a new waker or replaces the waker for an existing future.
    ///
    /// Returns the stable identifier assigned to the future.
    fn register(
        &mut self,
        registration_id: Option<RegistrationId>,
        waker: &Waker,
    ) -> RegistrationId {
        let registration_id =
            registration_id.unwrap_or_else(|| self.allocate_id());
        self.wakers.insert(registration_id, waker.clone());
        registration_id
    }

    /// Allocates an identifier that is not currently registered.
    fn allocate_id(&mut self) -> RegistrationId {
        loop {
            let candidate = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if !self.wakers.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    /// Removes the waker registered under the supplied identifier, if any.
    fn unregister(&mut self, registration_id: RegistrationId) {
        self.wakers.remove(&registration_id);
    }

    /// Removes and returns every registered waker.
    fn take_all(&mut self) -> Vec<Waker> {
        self.wakers.drain().map(|(_, waker)| waker).collect()
    }
}

/// A cloneable, runtime-independent cancellation token for retry executions.
///
/// Clones share cancellation state. A cancellation request is permanent and
/// wakes every future currently returned by
/// [`RetryCancellationToken::cancelled`].
#[derive(Clone, Debug, Default)]
pub struct RetryCancellationToken {
    /// State shared with cloned tokens and cancellation futures.
    inner: Arc<Inner>,
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
        if self.inner.cancelled.swap(true, Ordering::AcqRel) {
            return;
        }
        let wakers = {
            let mut waiters = self.inner.lock_waiters();
            waiters.take_all()
        };
        for waker in wakers {
            waker.wake();
        }
    }

    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// `true` after this token or any of its clones has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Creates a future that completes when cancellation is requested.
    ///
    /// # Returns
    /// A future borrowing this token. Dropping a pending future unregisters its
    /// waker.
    #[must_use]
    pub fn cancelled(&self) -> RetryCancelled<'_> {
        RetryCancelled {
            token: self,
            registration_id: None,
        }
    }
}

/// Future returned by [`RetryCancellationToken::cancelled`].
///
/// The future is runtime-independent and keeps at most one registered waker.
#[derive(Debug)]
pub struct RetryCancelled<'a> {
    /// Token whose cancellation state this future observes.
    token: &'a RetryCancellationToken,

    /// Stable registry entry allocated on the first pending poll.
    registration_id: Option<RegistrationId>,
}

impl Future for RetryCancelled<'_> {
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.token.is_cancelled() {
            return Poll::Ready(());
        }

        let mut waiters = this.token.inner.lock_waiters();
        let registration_id =
            waiters.register(this.registration_id, context.waker());
        this.registration_id = Some(registration_id);

        if this.token.inner.cancelled.load(Ordering::Acquire) {
            waiters.unregister(registration_id);
            this.registration_id = None;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for RetryCancelled<'_> {
    fn drop(&mut self) {
        if let Some(registration_id) = self.registration_id.take() {
            self.token.inner.lock_waiters().unregister(registration_id);
        }
    }
}
