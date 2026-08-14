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
    /// Registers an owned waker under the supplied stable identifier.
    ///
    /// Returns the replaced waker if the identifier was already registered.
    /// The caller must drop it after releasing the registry lock.
    fn register(
        &mut self,
        registration_id: RegistrationId,
        waker: Waker,
    ) -> Option<Waker> {
        self.wakers.insert(registration_id, waker)
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

    /// Removes and returns the waker registered under the supplied identifier.
    ///
    /// Returns `Some` when the identifier was registered and `None` otherwise.
    /// The caller must drop the returned waker after releasing the registry
    /// lock.
    fn unregister(&mut self, registration_id: RegistrationId) -> Option<Waker> {
        self.wakers.remove(&registration_id)
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

        let waker = context.waker().clone();
        let mut waiters = this.token.inner.lock_waiters();
        let registration_id = this
            .registration_id
            .unwrap_or_else(|| waiters.allocate_id());
        let replaced_waker = waiters.register(registration_id, waker);
        this.registration_id = Some(registration_id);

        if this.token.inner.cancelled.load(Ordering::Acquire) {
            let removed_waker = waiters.unregister(registration_id);
            this.registration_id = None;
            drop(waiters);
            drop(replaced_waker);
            drop(removed_waker);
            Poll::Ready(())
        } else {
            drop(waiters);
            drop(replaced_waker);
            Poll::Pending
        }
    }
}

impl Drop for RetryCancelled<'_> {
    fn drop(&mut self) {
        if let Some(registration_id) = self.registration_id.take() {
            let removed_waker = {
                let mut waiters = self.token.inner.lock_waiters();
                waiters.unregister(registration_id)
            };
            drop(removed_waker);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::ManuallyDrop;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::TryLockError;
    use std::sync::Weak;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::task::Context;
    use std::task::Poll;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;

    use super::Inner;
    use super::RetryCancellationToken;

    /// Identifies a raw-waker callback observed by a test probe.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReentrantWakerCallback {
        /// The raw waker was cloned.
        Clone,

        /// The raw waker was dropped.
        Drop,
    }

    /// Records whether one raw-waker callback ran while the registry was locked.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ReentrantWakerEvent {
        /// Callback that was invoked.
        callback: ReentrantWakerCallback,

        /// Whether the callback observed the registry lock as unavailable.
        registry_was_locked: bool,
    }

    /// Shared controls and event log for a reentrant raw waker.
    #[derive(Debug)]
    struct ReentrantWakerState {
        /// Token state inspected by raw-waker callbacks.
        inner: Weak<Inner>,

        /// Events emitted by callbacks selected for observation.
        events: Mutex<Vec<ReentrantWakerEvent>>,

        /// Whether clone callbacks should inspect and record registry state.
        observe_clone: AtomicBool,

        /// Whether drop callbacks should inspect and record registry state.
        observe_drop: AtomicBool,

        /// Whether a clone callback should request cancellation without locking.
        cancel_on_clone: AtomicBool,
    }

    impl ReentrantWakerState {
        /// Creates a probe associated with the supplied token state.
        fn new(inner: &Arc<Inner>) -> Self {
            Self {
                inner: Arc::downgrade(inner),
                events: Mutex::new(Vec::new()),
                observe_clone: AtomicBool::new(false),
                observe_drop: AtomicBool::new(false),
                cancel_on_clone: AtomicBool::new(false),
            }
        }

        /// Enables or disables clone-callback observation.
        fn set_observe_clone(&self, observe: bool) {
            self.observe_clone.store(observe, Ordering::SeqCst);
        }

        /// Enables or disables drop-callback observation.
        fn set_observe_drop(&self, observe: bool) {
            self.observe_drop.store(observe, Ordering::SeqCst);
        }

        /// Controls whether the next clone callbacks set the cancellation flag.
        fn set_cancel_on_clone(&self, cancel: bool) {
            self.cancel_on_clone.store(cancel, Ordering::SeqCst);
        }

        /// Removes and returns every event recorded so far.
        fn take_events(&self) -> Vec<ReentrantWakerEvent> {
            let mut events = self
                .events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *events)
        }

        /// Handles a raw-waker clone callback.
        fn on_clone(&self) {
            if self.cancel_on_clone.load(Ordering::SeqCst)
                && let Some(inner) = self.inner.upgrade()
            {
                inner.cancelled.store(true, Ordering::Release);
            }
            if self.observe_clone.load(Ordering::SeqCst) {
                self.record(ReentrantWakerCallback::Clone);
            }
        }

        /// Handles a raw-waker drop callback.
        fn on_drop(&self) {
            if self.observe_drop.load(Ordering::SeqCst) {
                self.record(ReentrantWakerCallback::Drop);
            }
        }

        /// Records whether the token registry is locked during a callback.
        fn record(&self, callback: ReentrantWakerCallback) {
            let inner = self
                .inner
                .upgrade()
                .expect("the cancellation token must outlive its test waker");
            let registry_was_locked = match inner.waiters.try_lock() {
                Ok(waiters) => {
                    drop(waiters);
                    false
                }
                Err(TryLockError::WouldBlock) => true,
                Err(TryLockError::Poisoned(poisoned)) => {
                    drop(poisoned.into_inner());
                    false
                }
            };
            self.events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(ReentrantWakerEvent {
                    callback,
                    registry_was_locked,
                });
        }
    }

    /// Raw-waker vtable used by deterministic registry-lock probes.
    static REENTRANT_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        clone_reentrant_waker,
        wake_reentrant_waker,
        wake_reentrant_waker_by_ref,
        drop_reentrant_waker,
    );

    /// Clones a raw waker while preserving its backing [`Arc`] invariant.
    unsafe fn clone_reentrant_waker(data: *const ()) -> RawWaker {
        // SAFETY: `data` originates from `Arc::into_raw` in
        // `reentrant_waker` or this function. `ManuallyDrop` keeps the strong
        // reference owned by the source raw waker alive, while `Arc::clone`
        // creates the strong reference transferred to the returned raw waker.
        let state = ManuallyDrop::new(unsafe {
            Arc::<ReentrantWakerState>::from_raw(data.cast())
        });
        state.on_clone();
        let cloned = Arc::clone(&state);
        RawWaker::new(Arc::into_raw(cloned).cast(), &REENTRANT_WAKER_VTABLE)
    }

    /// Consumes a raw waker without emitting a wake event.
    unsafe fn wake_reentrant_waker(data: *const ()) {
        // SAFETY: `data` represents one strong reference transferred to this
        // consuming callback by the `RawWaker` contract.
        drop(unsafe { Arc::<ReentrantWakerState>::from_raw(data.cast()) });
    }

    /// Borrows a raw waker without consuming its backing strong reference.
    unsafe fn wake_reentrant_waker_by_ref(_data: *const ()) {}

    /// Drops a raw waker and records registry-lock state before releasing it.
    unsafe fn drop_reentrant_waker(data: *const ()) {
        // SAFETY: `data` represents one strong reference transferred to this
        // consuming callback by the `RawWaker` contract.
        let state =
            unsafe { Arc::<ReentrantWakerState>::from_raw(data.cast()) };
        state.on_drop();
    }

    /// Creates a named reentrant waker and the state controlling its callbacks.
    fn reentrant_waker(
        token: &RetryCancellationToken,
    ) -> (Arc<ReentrantWakerState>, Waker) {
        let state = Arc::new(ReentrantWakerState::new(&token.inner));
        let raw_waker = RawWaker::new(
            Arc::into_raw(Arc::clone(&state)).cast(),
            &REENTRANT_WAKER_VTABLE,
        );
        // SAFETY: the vtable preserves exactly one `Arc` strong reference for
        // every live raw waker and consumes it in `wake` or `drop`.
        let waker = unsafe { Waker::from_raw(raw_waker) };
        (state, waker)
    }

    /// Verifies first registration clones the context waker outside the lock.
    #[test]
    fn test_cancelled_first_poll_does_not_clone_waker_under_registry_lock() {
        let token = RetryCancellationToken::new();
        let (state, waker) = reentrant_waker(&token);
        state.set_observe_clone(true);
        let mut cancelled = Box::pin(token.cancelled());
        let mut context = Context::from_waker(&waker);

        assert_eq!(Poll::Pending, cancelled.as_mut().poll(&mut context));

        assert_eq!(
            vec![ReentrantWakerEvent {
                callback: ReentrantWakerCallback::Clone,
                registry_was_locked: false,
            }],
            state.take_events(),
        );
    }

    /// Verifies re-poll replacement clones and drops wakers outside the lock.
    #[test]
    fn test_cancelled_repoll_replaces_waker_outside_registry_lock() {
        let token = RetryCancellationToken::new();
        let (state, waker) = reentrant_waker(&token);
        let mut cancelled = Box::pin(token.cancelled());
        let mut context = Context::from_waker(&waker);
        assert_eq!(Poll::Pending, cancelled.as_mut().poll(&mut context));
        state.take_events();
        state.set_observe_clone(true);
        state.set_observe_drop(true);

        assert_eq!(Poll::Pending, cancelled.as_mut().poll(&mut context));

        assert_eq!(
            vec![
                ReentrantWakerEvent {
                    callback: ReentrantWakerCallback::Clone,
                    registry_was_locked: false,
                },
                ReentrantWakerEvent {
                    callback: ReentrantWakerCallback::Drop,
                    registry_was_locked: false,
                },
            ],
            state.take_events(),
        );
    }

    /// Verifies second-check unregistration drops its waker outside the lock.
    #[test]
    fn test_cancelled_second_check_unregisters_waker_outside_registry_lock() {
        let token = RetryCancellationToken::new();
        let (state, waker) = reentrant_waker(&token);
        state.set_cancel_on_clone(true);
        state.set_observe_drop(true);
        let mut cancelled = Box::pin(token.cancelled());
        let mut context = Context::from_waker(&waker);

        assert_eq!(Poll::Ready(()), cancelled.as_mut().poll(&mut context));

        assert_eq!(
            vec![ReentrantWakerEvent {
                callback: ReentrantWakerCallback::Drop,
                registry_was_locked: false,
            }],
            state.take_events(),
        );
    }

    /// Verifies dropping a pending future drops its waker outside the lock.
    #[test]
    fn test_cancelled_drop_unregisters_waker_outside_registry_lock() {
        let token = RetryCancellationToken::new();
        let (state, waker) = reentrant_waker(&token);
        let mut cancelled = Box::pin(token.cancelled());
        let mut context = Context::from_waker(&waker);
        assert_eq!(Poll::Pending, cancelled.as_mut().poll(&mut context));
        state.take_events();
        state.set_observe_drop(true);

        drop(cancelled);

        assert_eq!(
            vec![ReentrantWakerEvent {
                callback: ReentrantWakerCallback::Drop,
                registry_was_locked: false,
            }],
            state.take_events(),
        );
    }
}
