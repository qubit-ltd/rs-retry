// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::thread;

use qubit_retry::{
    AttemptCancelToken,
    Retry,
};

use crate::support::TestError;

/// Verifies a type-erased blocking attempt receives a fresh token and runs on
/// its worker thread.
#[test]
fn test_blocking_attempt_runs_with_uncancelled_token_on_worker_thread() {
    let caller_thread = thread::current().id();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let worker_thread = retry
        .run_in_worker(|token: AttemptCancelToken| {
            assert!(!token.is_cancelled());
            Ok(thread::current().id())
        })
        .expect("blocking attempt should succeed");

    assert_ne!(worker_thread, caller_thread);
}
