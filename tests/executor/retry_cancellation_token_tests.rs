// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;

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

/// Verifies the default token starts in the non-cancelled state.
#[test]
fn test_retry_cancellation_token_default_starts_not_cancelled() {
    let token = RetryCancellationToken::default();

    assert!(!token.is_cancelled());
}

/// Verifies cancellation is visible through cloned tokens.
#[test]
fn test_retry_cancellation_token_cancel_is_shared_by_clones() {
    let token = RetryCancellationToken::new();
    let clone = token.clone();

    clone.cancel();

    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
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

/// Verifies cancellation before the first poll is observed without a wake.
#[test]
fn test_retry_cancellation_token_cancel_before_registration_is_ready() {
    let token = RetryCancellationToken::new();
    let (counter, waker) = counting_waker();

    token.cancel();
    let mut cancelled = Box::pin(token.cancelled());

    assert_eq!(Poll::Ready(()), poll_once(cancelled.as_mut(), &waker));
    assert_eq!(0, counter.count());
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

/// Verifies dropping a pending cancellation future unregisters its waker.
#[test]
fn test_retry_cancellation_token_drop_unregisters_waker() {
    let token = RetryCancellationToken::new();
    let (counter, waker) = counting_waker();
    {
        let mut cancelled = Box::pin(token.cancelled());
        assert_eq!(Poll::Pending, poll_once(cancelled.as_mut(), &waker));
    }

    token.cancel();

    assert_eq!(0, counter.count());
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
