// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared blocking backoff for synchronous retry executors.

use std::future::Future;
use std::sync::Arc;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::BlockingSleeper;
use qubit_clock::Timer;

use super::BlockingBackoffOutcome;
use super::blocking_backoff_wake::BlockingBackoffWake;
use crate::RetryCancellationToken;

/// Waits for one retry delay while optionally observing flow cancellation.
///
/// # Parameters
///
/// - `timer`: Timer used to register the blocking delay.
/// - `delay`: Selected backoff duration.
/// - `cancellation`: Optional flow cancellation token.
///
/// # Returns
///
/// Whether the delay elapsed, cancellation won, or the timer failed.
/// Cancellation is polled before the timer so it wins when both become ready
/// before the same wake cycle. Without a token, this function delegates to
/// [`BlockingSleeper`] to preserve the existing blocking path.
///
/// # Panics
/// Panics if all notification senders disappear while a pending future is
/// retained, which indicates an internal waker-lifetime invariant violation.
pub(crate) fn wait_for_backoff(
    timer: &Arc<dyn Timer>,
    delay: Duration,
    cancellation: Option<&RetryCancellationToken>,
) -> BlockingBackoffOutcome {
    let Some(token) = cancellation else {
        return match BlockingSleeper::new(Arc::clone(timer)).sleep_for(delay) {
            Ok(()) => BlockingBackoffOutcome::Elapsed,
            Err(error) => BlockingBackoffOutcome::TimerFailed(error),
        };
    };
    if token.is_cancelled() {
        return BlockingBackoffOutcome::Cancelled;
    }
    let mut timer_future = match timer.after(delay) {
        Ok(future) => future,
        Err(_) if token.is_cancelled() => {
            return BlockingBackoffOutcome::Cancelled;
        }
        Err(error) => return BlockingBackoffOutcome::TimerFailed(error),
    };
    let mut cancellation = Box::pin(token.cancelled());
    let (sender, receiver) = mpsc::channel();
    let waker = Waker::from(Arc::new(BlockingBackoffWake { sender }));
    let mut context = Context::from_waker(&waker);
    loop {
        if cancellation.as_mut().poll(&mut context).is_ready() {
            return BlockingBackoffOutcome::Cancelled;
        }
        if let Poll::Ready(result) = timer_future.as_mut().poll(&mut context) {
            return match result {
                Ok(()) => BlockingBackoffOutcome::Elapsed,
                Err(error) => BlockingBackoffOutcome::TimerFailed(error),
            };
        }
        receiver.recv().expect("backoff futures must retain their shared waker");
    }
}
