// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptExecutorError;
use qubit_retry::AttemptFailure;
use qubit_retry::AttemptFailureKind;
use qubit_retry::AttemptPanic;

use crate::support::TestError;

/// Verifies attempt failure accessors return application errors only for error
/// variants.
#[test]
fn test_attempt_failure_error_accessors_distinguish_timeout() {
    let failure = AttemptFailure::Error(TestError("boom"));
    assert_eq!(failure.as_error(), Some(&TestError("boom")));
    assert_eq!(failure.as_panic(), None);
    assert_eq!(failure.into_error(), Some(TestError("boom")));

    let timeout = AttemptFailure::<TestError>::Timeout;
    assert_eq!(timeout.as_error(), None);
    assert_eq!(timeout.as_executor_error(), None);
    assert_eq!(timeout.as_panic(), None);
    assert_eq!(timeout.into_error(), None);

    let panic = AttemptFailure::<TestError>::Panic(AttemptPanic::new("worker failed"));
    assert_eq!(panic.as_error(), None);
    assert_eq!(panic.as_executor_error(), None);
    assert_eq!(
        panic
            .as_panic()
            .expect("panic failure should expose captured panic")
            .message(),
        "worker failed"
    );
    assert_eq!(panic.into_error(), None);

    let executor =
        AttemptFailure::<TestError>::Executor(AttemptExecutorError::new("worker spawn failed"));
    assert_eq!(executor.as_error(), None);
    assert_eq!(
        executor
            .as_executor_error()
            .expect("executor failure should expose executor error")
            .message(),
        "worker spawn failed"
    );
    assert_eq!(executor.as_panic(), None);
    assert_eq!(executor.into_error(), None);
}

/// Verifies attempt failure display output for error and timeout variants.
#[test]
fn test_attempt_failure_display_formats_variants() {
    assert_eq!(
        AttemptFailure::Error(TestError("operation failed")).to_string(),
        "operation failed"
    );
    assert_eq!(
        AttemptFailure::<TestError>::Timeout.to_string(),
        "attempt timed out"
    );
    assert_eq!(
        AttemptFailure::<TestError>::Panic(AttemptPanic::new("worker failed")).to_string(),
        "attempt panicked: worker failed"
    );
    assert_eq!(
        AttemptFailure::<TestError>::Executor(AttemptExecutorError::new("worker spawn failed"))
            .to_string(),
        "attempt executor failed: worker spawn failed"
    );
}

/// Verifies failure kinds and timeout classification are stable across
/// variants.
#[test]
fn test_attempt_failure_kind_and_timeout_classification() {
    let error = AttemptFailure::Error(TestError("boom"));
    assert_eq!(error.kind(), AttemptFailureKind::Application);
    assert!(!error.is_timeout());

    let timeout = AttemptFailure::<TestError>::Timeout;
    assert_eq!(timeout.kind(), AttemptFailureKind::TimedOut);
    assert!(timeout.is_timeout());

    let panic = AttemptFailure::<TestError>::Panic(AttemptPanic::new("panic"));
    assert_eq!(panic.kind(), AttemptFailureKind::Panicked);
    assert!(!panic.is_timeout());

    let executor = AttemptFailure::<TestError>::Executor(AttemptExecutorError::new("executor"));
    assert_eq!(executor.kind(), AttemptFailureKind::Infrastructure);
    assert!(!executor.is_timeout());
}
