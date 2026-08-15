// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Terminal retry errors are covered through public executor outcomes.

use std::error::Error;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

/// Verifies the public retry error view is lossless and exposes the last
/// application error as its standard error source.
#[test]
fn test_retry_error_preserves_terminal_failure_and_context() {
    let retry = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Abort
    })
    .build();

    let error = retry
        .sync()
        .run(|| Err::<(), _>(TestError("fatal")))
        .expect_err("the rule should abort the retry flow");

    assert!(matches!(
        error.failure(),
        RetryFailure::Aborted {
            last_failure: AttemptFailure::Error(TestError("fatal")),
            ..
        }
    ));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("fatal")));
    assert_eq!(
        Error::source(&error).map(ToString::to_string),
        Some("fatal".to_owned())
    );
    assert_eq!(error.to_string(), "retry aborted: fatal after 1 attempt(s)");

    let (failure, context) = error.into_parts();
    assert!(matches!(
        failure,
        RetryFailure::Aborted {
            last_failure: AttemptFailure::Error(TestError("fatal")),
            ..
        }
    ));
    assert_eq!(context.attempts(), 1);
}
