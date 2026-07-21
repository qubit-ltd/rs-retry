// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use qubit_retry::{AttemptCancelToken, Retry, RetryErrorReason};

/// Verifies worker-attempt execution is observable through the public worker
/// API.
#[test]
fn test_worker_attempt_executor_paths_are_observable_through_run_in_worker() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .attempt_timeout(Some(Duration::from_millis(5)))
        .worker_cancel_grace(Duration::ZERO)
        .no_delay()
        .build()
        .expect("retry should build");

    let value = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    Err("retry")
                } else {
                    Ok("done")
                }
            }
        })
        .expect("ordinary worker error should retry");

    assert_eq!(value, "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
    let error = retry
        .run_in_worker(move |_token: AttemptCancelToken| {
            release_rx
                .lock()
                .expect("release receiver should be lockable")
                .recv()
                .expect("test should release the worker");
            finished_tx
                .send(())
                .expect("worker completion should be observable");
            Ok::<_, &'static str>("late")
        })
        .expect_err("uncooperative timed-out worker should stop retrying");
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(error.unreaped_worker_count(), 1);
}
