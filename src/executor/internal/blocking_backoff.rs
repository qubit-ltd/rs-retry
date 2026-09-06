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
use std::task::Wake;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::BlockingSleeper;
use qubit_clock::Timer;

use super::BlockingBackoffOutcome;
use crate::RetryCancellationToken;

/// Waker that forwards timer and cancellation notifications to one channel.
struct BlockingBackoffWake {
    /// Notification sender shared by both futures.
    sender: mpsc::Sender<()>,
}

impl Wake for BlockingBackoffWake {
    /// Wakes the blocking retry thread through its notification channel.
    fn wake(self: Arc<Self>) {
        let _ = self.sender.send(());
    }

    /// Wakes the blocking retry thread without consuming the shared waker.
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.sender.send(());
    }
}

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
