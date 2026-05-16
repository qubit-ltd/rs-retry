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

/// Verifies blocking attempt outcome cleanup counts through public retry errors.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_blocking_attempt_outcome_reports_unreaped_worker_count() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .attempt_timeout(Some(Duration::from_millis(10)))
        .worker_cancel_grace(Duration::ZERO)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            std::thread::sleep(Duration::from_millis(80));
            Err::<(), &'static str>("late")
        })
        .expect_err("unreaped worker should stop retrying");

    assert_eq!(RetryErrorReason::WorkerStillRunning, error.reason());
    assert_eq!(1, error.unreaped_worker_count());
    assert_eq!(1, error.context().unreaped_worker_count());
}
