// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
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
    struct AbortRule(Arc<AtomicUsize>);
    impl RetryRule<UnitTestError> for AbortRule {
        fn decide(&self, _: &AttemptFailure<UnitTestError>, _: &RetryContext) -> RetryDecision {
            self.0.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Abort
        }
    }
    let later_calls = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .rule(RetryOnly)
        .rule(AbortRule(Arc::clone(&later_calls)))
        .build()
        .expect("valid config");
    let error = Retry::new(&retry).run::<(), _>(|| Err(UnitTestError)).unwrap_err();
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
    assert_eq!(later_calls.load(Ordering::SeqCst), 0);

    let calibration = RetryConfig::<UnitTestError>::builder()
        .rule(AbortRule(Arc::clone(&later_calls)))
        .build()
        .expect("valid calibration config");
    let calibration_error = Retry::new(&calibration)
        .run::<(), _>(|| Err(UnitTestError))
        .expect_err("the first abort rule stops the flow");
    assert!(matches!(calibration_error.reason(), RetryErrorReason::Aborted));
    assert_eq!(later_calls.load(Ordering::SeqCst), 1);
}
