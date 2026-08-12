// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::thread;

use qubit_retry::AttemptCancelToken;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

/// Verifies a type-erased blocking attempt receives a fresh token and runs on
/// its worker thread.
#[test]
fn test_blocking_attempt_runs_with_uncancelled_token_on_worker_thread() {
    let caller_thread = thread::current().id();
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("retry should build");
    let retry = Retry::<TestError>::builder(policy).build();

    let worker_thread = retry
        .worker()
        .run(|token: AttemptCancelToken| {
            assert!(!token.is_cancelled());
            Ok(thread::current().id())
        })
        .expect("blocking attempt should succeed");

    assert_ne!(worker_thread.into_value(), caller_thread);
}
