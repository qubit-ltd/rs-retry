// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptCancelToken;
use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorKind;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryPolicy;
use qubit_retry::error::RetryExecutionErrorKind;

use crate::support::TestError;

#[test]
fn worker_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.worker();
}

#[test]
fn worker_spawn_failure_preserves_infrastructure_diagnostic() {
    let operation_calls = std::sync::Arc::new(AtomicUsize::new(0));
    let rule_calls = std::sync::Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy)
        .rule({
            let rule_calls = std::sync::Arc::clone(&rule_calls);
            move |_: &AttemptFailure<TestError>, _: &RetryContext| {
                rule_calls.fetch_add(1, Ordering::SeqCst);
                RetryDecision::Retry
            }
        })
        .build();

    let error = retry
        .worker()
        .worker_stack_size(usize::MAX)
        .run({
            let operation_calls = std::sync::Arc::clone(&operation_calls);
            move |_: AttemptCancelToken| {
                operation_calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(TestError("operation should not run"))
            }
        })
        .unwrap_err();

    assert_eq!(operation_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
    assert_eq!(error.reason(), RetryErrorReason::WorkerFailed);
    assert_eq!(error.kind(), RetryErrorKind::Infrastructure);
    assert_eq!(
        error
            .execution_error()
            .expect("worker spawn error should be preserved")
            .kind(),
        RetryExecutionErrorKind::Worker
    );
    assert!(
        !error
            .execution_error()
            .expect("worker spawn error should be preserved")
            .message()
            .is_empty()
    );
}
