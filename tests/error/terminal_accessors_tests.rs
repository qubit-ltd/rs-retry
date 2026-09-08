// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryError;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;

use crate::support::UnitTestError;

/// Runs one failing operation to produce a public terminal retry error.
fn create_exhausted_error() -> RetryError<UnitTestError> {
    Retry::<UnitTestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("terminal-accessor policy should be valid"),
    )
    .fallback(RetryFallback::Retry)
    .build()
    .sync()
    .run::<(), _>(|| Err(UnitTestError))
    .expect_err("one failed attempt should exhaust the policy")
}

/// Verifies public retry-error accessors retain all terminal information.
#[test]
fn test_retry_error_terminal_accessors_are_lossless() {
    let error = create_exhausted_error();
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts
        }
    ));
    assert_eq!(error.last_error(), Some(&UnitTestError));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Error(UnitTestError))
    ));

    let error = create_exhausted_error();
    let (reason, failure, context, diagnostics) = error.into_parts();
    assert!(diagnostics.is_empty());
    assert!(matches!(
        reason,
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts
        }
    ));
    assert_eq!(
        failure.as_ref().and_then(AttemptFailure::as_error),
        Some(&UnitTestError)
    );
    assert_eq!(context.attempts(), 1);
    assert_eq!(context.current_attempt(), None);
}
