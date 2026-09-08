// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared state for retry-cancellation tokens and pending futures.

use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Waker;

use super::waker_registry::WakerRegistry;

/// Shared cancellation state owned by every clone of a token.
#[derive(Debug, Default)]
pub(in crate::executor) struct RetryCancellationState {
    /// Whether cancellation has been requested.
    cancelled: AtomicBool,
    /// Wakers waiting for the first cancellation request.
    waiters: Mutex<WakerRegistry>,
}

impl RetryCancellationState {
    /// Returns whether cancellation has been requested.
    ///
    /// # Returns
    /// True after the first release of a cancellation request.
    #[inline(always)]
    #[must_use]
    pub(in crate::executor) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Requests cancellation and wakes every currently registered waiter.
    ///
    /// Wakers leave the registry before invocation, so re-entrant callbacks
    /// cannot observe the registry mutex as locked.
    pub(in crate::executor) fn cancel(&self) {
        if self.cancelled.swap(true, Ordering::AcqRel) {
            return;
        }
        let wakers = self.lock_waiters().take_all();
        for waker in wakers {
            waker.wake();
        }
    }

    /// Registers a future waker and returns its stable registration identifier.
    ///
    /// Returned wakers must be dropped after this method returns, when the
    /// registry mutex has been released.
    ///
    /// # Parameters
    /// - `registration_id`: Existing future entry, or None for its first
    ///   pending poll.
    /// - `waker`: Current task waker transferred into the registry.
    ///
    /// # Returns
    /// The stable identifier, Some replaced waker or None on first
    /// registration, Some removed waker if cancellation raced with
    /// registration or None otherwise, and whether cancellation has been
    /// observed.
    pub(in crate::executor) fn register(
        &self,
        registration_id: Option<u64>,
        waker: Waker,
    ) -> (u64, Option<Waker>, Option<Waker>, bool) {
        let mut waiters = self.lock_waiters();
        let (registration_id, replaced) = waiters.register(registration_id, waker);
        let cancelled = self.cancelled.load(Ordering::Acquire);
        let removed = cancelled.then(|| waiters.unregister(registration_id)).flatten();
        (registration_id, replaced, removed, cancelled)
    }

    /// Unregisters a pending cancellation future.
    ///
    /// The returned waker must be dropped after the registry mutex is released.
    ///
    /// # Parameters
    /// - `registration_id`: Entry owned by the dropping future.
    ///
    /// # Returns
    /// Some removed waker, or None if cancellation already drained it.
    #[inline(always)]
    pub(in crate::executor) fn unregister(&self, registration_id: u64) -> Option<Waker> {
        self.lock_waiters().unregister(registration_id)
    }

    /// Locks the waker registry, recovering its contents after poisoning.
    ///
    /// # Returns
    /// An exclusive guard; poisoning preserves the registry for cleanup.
    #[inline]
    fn lock_waiters(&self) -> MutexGuard<'_, WakerRegistry> {
        self.waiters.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
