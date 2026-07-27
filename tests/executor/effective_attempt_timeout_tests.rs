// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken,
    AttemptTimeoutOption,
    AttemptTimeoutSource,
    Retry,
    RetryErrorReason,
};

/// Verifies effective attempt timeout source selection through public retry
/// behavior.
#[test]
fn test_effective_attempt_timeout_configured_source_wins_equal_elapsed_budget()
{
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(20),
        )))
        .worker_cancel_grace(Duration::from_millis(100))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|token: AttemptCancelToken| {
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), &'static str>("cancelled")
        })
        .expect_err("configured timeout should abort");

    assert_eq!(RetryErrorReason::Aborted, error.reason());
    assert_eq!(
        Some(AttemptTimeoutSource::Configured),
        error.context().attempt_timeout_source()
    );
}
