// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::resume_unwind;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use qubit_retry::AttemptCancellationToken;

/// Bounds a worker test independently of the injected retry clock.
pub(crate) fn run_worker_test<T, F>(test: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let deadline = Instant::now() + Duration::from_secs(10);
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(test));
        let _ = sender.send(result);
    });
    let result = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("worker test exceeded its real-time limit or lost its runner");
    while !handle.is_finished() {
        assert!(Instant::now() < deadline, "worker test runner did not exit");
        thread::yield_now();
    }
    handle
        .join()
        .expect("worker test runner panicked outside its test body");
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

/// Releases a stuck test operation without relying on timer cancellation.
pub(crate) fn wait_for_cancellation(token: &AttemptCancellationToken, watchdog_fired: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !token.is_cancelled() {
        if Instant::now() >= deadline {
            watchdog_fired.store(true, Ordering::SeqCst);
            return;
        }
        thread::yield_now();
    }
}
