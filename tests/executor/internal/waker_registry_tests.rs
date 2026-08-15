// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public cancellation-waker ownership behavior.

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

#[derive(Default)]
struct WakeCounter(AtomicUsize);

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Verifies dropping a pending public future removes its stored waker.
#[test]
fn test_waker_registry_unregisters_a_dropped_cancellation_future() {
    let token = RetryCancellationToken::new();
    let counter = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&counter));
    {
        let mut future = Box::pin(token.cancelled());
        let mut context = Context::from_waker(&waker);
        assert_eq!(Pin::as_mut(&mut future).poll(&mut context), Poll::Pending);
    }
    token.cancel();
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
}
