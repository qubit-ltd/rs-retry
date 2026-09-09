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
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryLimitKind;

use crate::support::TestError;

/// Verifies terminal controller paths clear their public attempt overlay.
#[test]
fn test_retry_flow_controller_clears_attempt_scope_for_abort_and_limit() {
    let config = RetryConfig::<TestError>::builder()
        .max_attempts(2)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Abort)
        .build()
        .expect("valid config");
    let aborted = Retry::new(&config)
        .run(|| Err::<(), _>(TestError("abort")))
        .expect_err("the abort rule must terminate the flow");
    assert!(matches!(aborted.reason(), RetryErrorReason::Aborted));
    assert_eq!(aborted.context().current_attempt(), None);
    assert_eq!(aborted.context().current_hard_attempt_timeout(), None);

    let config2 = RetryConfig::<TestError>::builder()
        .max_attempts(1)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let exhausted = Retry::new(&config2)
        .run(|| Err::<(), _>(TestError("limit")))
        .expect_err("one failed operation must exhaust the attempt limit");
    assert!(matches!(
        exhausted.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));
    assert_eq!(exhausted.context().current_attempt(), None);
    assert_eq!(exhausted.context().current_hard_attempt_timeout(), None);
}
