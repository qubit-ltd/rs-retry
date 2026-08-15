// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public terminal-flow behavior.

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

/// Verifies terminal controller paths clear their public attempt overlay.
#[test]
fn test_retry_flow_controller_clears_attempt_scope_for_abort_and_limit() {
    let aborted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Abort
    })
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("abort")))
    .expect_err("the abort rule must terminate the flow");
    assert!(matches!(aborted.failure(), RetryFailure::Aborted { .. }));
    assert_eq!(aborted.context().current_attempt(), None);
    assert_eq!(aborted.context().current_attempt_timeout(), None);

    let exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("limit")))
    .expect_err("one failed operation must exhaust the attempt limit");
    assert!(matches!(
        exhausted.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));
    assert_eq!(exhausted.context().current_attempt(), None);
    assert_eq!(exhausted.context().current_attempt_timeout(), None);
}
