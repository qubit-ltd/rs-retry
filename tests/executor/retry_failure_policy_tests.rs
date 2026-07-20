// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use qubit_retry::{
    AttemptCancelToken,
    AttemptFailure,
    Retry,
    RetryErrorReason,
};

use crate::support::TestError;

/// Verifies the default failure policy aborts a captured worker panic without
/// starting another attempt.
#[test]
fn test_retry_failure_policy_aborts_worker_panic_by_default() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| -> Result<(), TestError> {
                attempts.fetch_add(1, Ordering::SeqCst);
                panic!("worker failed");
            }
        })
        .expect_err("worker panic should abort by default");

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    let panic = error
        .last_failure()
        .and_then(AttemptFailure::as_panic)
        .expect("terminal failure should be a captured panic");
    assert_eq!(panic.message(), "worker failed");
}
