/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken,
    Retry,
    RetryErrorReason,
};

/// Verifies worker runner result and timeout paths through public APIs.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_blocking_attempt_runner_paths_are_observable_through_blocking_timeout_and_success() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(20)))
        .worker_cancel_grace(Duration::from_millis(20))
        .build()
        .expect("retry should build");

    assert_eq!(
        "ok",
        retry
            .run_in_worker(|_token: AttemptCancelToken| Ok("ok"))
            .expect("worker attempt should succeed")
    );

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            std::thread::sleep(Duration::from_millis(100));
            Err::<&'static str, &'static str>("late")
        })
        .unwrap_err();
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
}
