// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;

use crate::support::UnitTestError;

#[test]
fn test_first_rule_wins_and_failure_kind_is_stable() {
    struct RetryOnly;
    impl RetryRule<UnitTestError> for RetryOnly {
        fn decide(&self, _: &AttemptFailure<UnitTestError>, _: &RetryContext) -> RetryDecision {
            RetryDecision::Retry
        }
    }
    struct AbortRule;
    impl RetryRule<UnitTestError> for AbortRule {
        fn decide(&self, _: &AttemptFailure<UnitTestError>, _: &RetryContext) -> RetryDecision {
            RetryDecision::Abort
        }
    }
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<UnitTestError>::builder(policy)
        .rule(RetryOnly)
        .rule(AbortRule)
        .build();
    let error = retry.sync().run::<(), _>(|| Err(UnitTestError)).unwrap_err();
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts,
        }
    ));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Error(UnitTestError))
    ));
}
