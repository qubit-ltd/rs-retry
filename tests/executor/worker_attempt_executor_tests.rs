/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::thread;
use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken,
    Retry,
    RetryErrorReason,
};

/// Verifies worker-attempt execution is observable through the public worker API.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
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

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            thread::sleep(Duration::from_millis(50));
            Ok::<_, &'static str>("late")
        })
        .expect_err("uncooperative timed-out worker should stop retrying");
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(error.unreaped_worker_count(), 1);
}
