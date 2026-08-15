// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::RawWaker;
use std::task::RawWakerVTable;
use std::task::Wake;
use std::task::Waker;
use std::thread;
use std::time::Duration;

use qubit_retry::RetryCancellationToken;

/// Counts calls made through an associated [`Waker`].
#[derive(Debug, Default)]
struct WakeCounter {
    /// Number of wake notifications observed so far.
    wakes: AtomicUsize,
}

impl WakeCounter {
    /// Returns the number of wake notifications observed so far.
    fn count(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }
}

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

/// Creates a counter and a waker that records notifications in that counter.
fn counting_waker() -> (Arc<WakeCounter>, Waker) {
    let counter = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&counter));
    (counter, waker)
}

/// Polls a cancellation future once with the supplied waker.
fn poll_once<F>(future: Pin<&mut F>, waker: &Waker) -> Poll<()>
where
    F: Future<Output = ()>,
{
    let mut context = Context::from_waker(waker);
    future.poll(&mut context)
}

/// Raw-waker callbacks that request cancellation while cloning a context waker.
static CANCEL_ON_CLONE_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    clone_cancel_on_clone_waker,
    wake_cancel_on_clone_waker,
    wake_by_ref_cancel_on_clone_waker,
    drop_cancel_on_clone_waker,
);

/// Clones a cancellation waker after requesting public token cancellation.
unsafe fn clone_cancel_on_clone_waker(data: *const ()) -> RawWaker {
    // SAFETY: `data` owns one strong reference created by
    // `cancel_on_clone_waker`; `ManuallyDrop` retains that source reference
    // while the cloned reference is transferred to the returned raw waker.
    let token = ManuallyDrop::new(unsafe {
        Arc::<RetryCancellationToken>::from_raw(data.cast())
    });
    token.cancel();
    let cloned = Arc::clone(&token);
    RawWaker::new(Arc::into_raw(cloned).cast(), &CANCEL_ON_CLONE_WAKER_VTABLE)
}

/// Consumes a cancellation waker without adding observable work.
unsafe fn wake_cancel_on_clone_waker(data: *const ()) {
    // SAFETY: `data` owns the strong reference transferred to this consuming
    // callback by the `RawWaker` contract.
    drop(unsafe { Arc::<RetryCancellationToken>::from_raw(data.cast()) });
}

/// Borrows a cancellation waker without consuming its strong reference.
unsafe fn wake_by_ref_cancel_on_clone_waker(_data: *const ()) {}

/// Drops a cancellation waker's transferred strong reference.
unsafe fn drop_cancel_on_clone_waker(data: *const ()) {
    // SAFETY: `data` owns the strong reference transferred to this consuming
    // callback by the `RawWaker` contract.
    drop(unsafe { Arc::<RetryCancellationToken>::from_raw(data.cast()) });
}

/// Creates a waker that cancels the supplied token when it is cloned.
fn cancel_on_clone_waker(token: &RetryCancellationToken) -> Waker {
    let token = Arc::new(token.clone());
    let raw_waker = RawWaker::new(
        Arc::into_raw(token).cast(),
        &CANCEL_ON_CLONE_WAKER_VTABLE,
    );
    // SAFETY: the callback table maintains exactly one `Arc` strong reference
    // for each raw waker and consumes it in `wake` or `drop`.
    unsafe { Waker::from_raw(raw_waker) }
}

/// Raw-waker callbacks whose destruction re-enters public cancellation.
static DROP_CANCEL_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    clone_drop_cancel_waker,
    wake_drop_cancel_waker,
    wake_by_ref_drop_cancel_waker,
    drop_drop_cancel_waker,
);

/// Clones a re-entrant cancellation waker while retaining its source reference.
unsafe fn clone_drop_cancel_waker(data: *const ()) -> RawWaker {
    // SAFETY: `data` owns one strong reference created by
    // `drop_cancelling_waker`; `ManuallyDrop` retains the source reference
    // while the cloned reference is transferred to the returned raw waker.
    let token = ManuallyDrop::new(unsafe {
        Arc::<RetryCancellationToken>::from_raw(data.cast())
    });
    let cloned = Arc::clone(&token);
    RawWaker::new(Arc::into_raw(cloned).cast(), &DROP_CANCEL_WAKER_VTABLE)
}

/// Consumes a re-entrant cancellation waker without cancellation.
unsafe fn wake_drop_cancel_waker(data: *const ()) {
    // SAFETY: `data` owns the strong reference transferred to this consuming
    // callback by the `RawWaker` contract.
    drop(unsafe { Arc::<RetryCancellationToken>::from_raw(data.cast()) });
}

/// Borrows a re-entrant cancellation waker without consuming its reference.
unsafe fn wake_by_ref_drop_cancel_waker(_data: *const ()) {}

/// Drops a waker after re-entering public token cancellation.
unsafe fn drop_drop_cancel_waker(data: *const ()) {
    // SAFETY: `data` owns the strong reference transferred to this consuming
    // callback by the `RawWaker` contract.
    let token = unsafe { Arc::<RetryCancellationToken>::from_raw(data.cast()) };
    token.cancel();
}

/// Creates a waker that cancels `token` whenever a cloned waker is dropped.
fn drop_cancelling_waker(token: &RetryCancellationToken) -> Waker {
    let token = Arc::new(token.clone());
    let raw_waker =
        RawWaker::new(Arc::into_raw(token).cast(), &DROP_CANCEL_WAKER_VTABLE);
    // SAFETY: the callback table maintains exactly one `Arc` strong reference
    // for each raw waker and consumes it in `wake` or `drop`.
    unsafe { Waker::from_raw(raw_waker) }
}

/// Asserts a controlled re-entrant callback completed without deadlocking.
fn assert_reentrant_callback_completes(
    receiver: mpsc::Receiver<bool>,
    handle: thread::JoinHandle<()>,
    callback: &str,
) {
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => handle
            .join()
            .expect("successful re-entrant callback thread must not panic"),
        Ok(false) => {
            drop(handle);
            panic!("the controlled {callback} poll must be pending");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            drop(handle);
            panic!("re-entrant {callback} must not retain the registry lock");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            drop(handle);
            panic!("re-entrant {callback} thread disconnected before success");
        }
    }
}

/// Verifies the default token starts in the non-cancelled state.
#[test]
fn test_retry_cancellation_token_default_starts_not_cancelled() {
    let token = RetryCancellationToken::default();

    assert!(!token.is_cancelled());
}

/// Verifies repeated cancellation requests wake a registered waiter once.
#[test]
fn test_retry_cancellation_token_cancel_is_idempotent() {
    let token = RetryCancellationToken::new();
    let (counter, waker) = counting_waker();
    let mut cancelled = Box::pin(token.cancelled());
    assert_eq!(Poll::Pending, poll_once(cancelled.as_mut(), &waker));

    token.cancel();
    token.cancel();

    assert_eq!(1, counter.count());
    assert_eq!(Poll::Ready(()), poll_once(cancelled.as_mut(), &waker));
}

/// Verifies one cancellation wakes every waiter registered with the token.
#[test]
fn test_retry_cancellation_token_cancel_wakes_all_waiters() {
    let token = RetryCancellationToken::new();
    let (first_counter, first_waker) = counting_waker();
    let (second_counter, second_waker) = counting_waker();
    let mut first = Box::pin(token.cancelled());
    let mut second = Box::pin(token.cancelled());
    assert_eq!(Poll::Pending, poll_once(first.as_mut(), &first_waker));
    assert_eq!(Poll::Pending, poll_once(second.as_mut(), &second_waker));

    token.cancel();

    assert_eq!(1, first_counter.count());
    assert_eq!(1, second_counter.count());
    assert_eq!(Poll::Ready(()), poll_once(first.as_mut(), &first_waker));
    assert_eq!(Poll::Ready(()), poll_once(second.as_mut(), &second_waker));
}

/// Verifies cancellation after registration wakes and completes the waiter.
#[test]
fn test_retry_cancellation_token_cancel_after_registration_wakes_waiter() {
    let token = RetryCancellationToken::new();
    let (counter, waker) = counting_waker();
    let mut cancelled = Box::pin(token.cancelled());
    assert_eq!(Poll::Pending, poll_once(cancelled.as_mut(), &waker));

    token.cancel();

    assert_eq!(1, counter.count());
    assert_eq!(Poll::Ready(()), poll_once(cancelled.as_mut(), &waker));
}

/// Verifies repeated polls replace a future's existing waker registration.
#[test]
fn test_retry_cancellation_token_repoll_updates_existing_registration() {
    let token = RetryCancellationToken::new();
    let (stale_counter, stale_waker) = counting_waker();
    let (current_counter, current_waker) = counting_waker();
    let mut cancelled = Box::pin(token.cancelled());

    assert_eq!(Poll::Pending, poll_once(cancelled.as_mut(), &stale_waker));
    assert_eq!(Poll::Pending, poll_once(cancelled.as_mut(), &current_waker));
    token.cancel();

    assert_eq!(0, stale_counter.count());
    assert_eq!(1, current_counter.count());
}

/// Verifies public cancellation during waker cloning completes the future.
#[test]
fn test_retry_cancellation_token_cancellation_during_registration_is_ready() {
    let token = RetryCancellationToken::new();
    let thread_token = token.clone();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let waker = cancel_on_clone_waker(&thread_token);
        let mut cancelled = Box::pin(thread_token.cancelled());
        sender
            .send(poll_once(cancelled.as_mut(), &waker) == Poll::Ready(()))
            .expect("test receiver must remain available");
    });

    assert_reentrant_callback_completes(receiver, handle, "waker clone");
    assert!(token.is_cancelled());
}

/// Verifies replacing a registered waker drops it after unlocking the registry.
#[test]
fn test_retry_cancellation_token_repoll_drop_can_reenter_cancellation() {
    let token = RetryCancellationToken::new();
    let thread_token = token.clone();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let stale_waker = drop_cancelling_waker(&thread_token);
        let mut cancelled = Box::pin(thread_token.cancelled());
        let first = poll_once(cancelled.as_mut(), &stale_waker);
        let second = poll_once(cancelled.as_mut(), Waker::noop());
        sender
            .send(first == Poll::Pending && second == Poll::Pending)
            .expect("test receiver must remain available");
    });

    assert_reentrant_callback_completes(
        receiver,
        handle,
        "waker replacement drop",
    );
    assert!(token.is_cancelled());
    let mut cancelled = Box::pin(token.cancelled());
    assert_eq!(
        Poll::Ready(()),
        poll_once(cancelled.as_mut(), Waker::noop())
    );
}

/// Verifies dropping a pending future drops its waker after unlocking the
/// registry.
#[test]
fn test_retry_cancellation_token_drop_can_reenter_cancellation() {
    let token = RetryCancellationToken::new();
    let thread_token = token.clone();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let waker = drop_cancelling_waker(&thread_token);
        let mut cancelled = Box::pin(thread_token.cancelled());
        let pending = poll_once(cancelled.as_mut(), &waker) == Poll::Pending;
        drop(cancelled);
        sender
            .send(pending)
            .expect("test receiver must remain available");
    });

    assert_reentrant_callback_completes(
        receiver,
        handle,
        "pending future drop",
    );
    assert!(token.is_cancelled());
    let mut cancelled = Box::pin(token.cancelled());
    assert_eq!(
        Poll::Ready(()),
        poll_once(cancelled.as_mut(), Waker::noop())
    );
}
