// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public cancellation-future behavior.

use std::future::Future;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_retry::RetryCancellationToken;

#[test]
fn test_retry_cancelled_is_ready_after_public_cancellation() {
    let token = RetryCancellationToken::new();
    token.cancel();
    let mut future = Box::pin(token.cancelled());
    let waker = Waker::noop();
    assert_eq!(
        future.as_mut().poll(&mut Context::from_waker(waker)),
        Poll::Ready(())
    );
}
