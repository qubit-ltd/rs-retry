// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use qubit_retry::{AttemptCancelToken, Retry, RetryErrorReason};

/// Verifies blocking attempt outcome cleanup counts through public retry
/// errors.
#[test]
fn test_blocking_attempt_outcome_reports_unreaped_worker_count() {
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .attempt_timeout(Some(Duration::from_millis(10)))
        .worker_cancel_grace(Duration::ZERO)
        .no_delay()
        .build()
        .expect("retry should build");

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
            Err::<(), &'static str>("late")
        })
        .expect_err("unreaped worker should stop retrying");
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");

    assert_eq!(RetryErrorReason::WorkerStillRunning, error.reason());
    assert_eq!(1, error.unreaped_worker_count());
    assert_eq!(1, error.context().unreaped_worker_count());
}
