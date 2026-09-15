// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryBudgetError;
use qubit_retry::RetryBudgetExhausted;
use qubit_retry::RetryPolicy;

#[test]
fn test_total_budget_rejects_delay_reaching_deadline() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .total_time_budget(Duration::from_secs(5))
        .build()
        .expect("retry policy should be valid");
    let mut budget = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("manual clock should represent the deadline");

    assert!(matches!(
        budget.check_retry_after(Duration::from_secs(5)),
        Err(RetryBudgetError::Exhausted(
            RetryBudgetExhausted::TotalElapsed
        ))
    ));
}

#[test]
fn test_operation_overrun_only_blocks_future_attempts() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .operation_time_budget(Duration::from_secs(1))
        .build()
        .expect("retry policy should be valid");
    let mut budget = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("manual clock should represent the policy");
    let attempt = budget.begin_attempt().expect("first attempt is admitted");
    clock
        .advance(Duration::from_secs(2))
        .expect("manual clock should advance");

    let snapshot = budget.finish_attempt(attempt).expect("valid completion");

    assert_eq!(snapshot.attempts(), 1);
    assert_eq!(snapshot.operation_elapsed(), Duration::from_secs(2));
    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetError::Exhausted(
            RetryBudgetExhausted::OperationElapsed
        ))
    ));
}

#[test]
fn test_attempt_budget_is_the_only_attempt_counter() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .build()
        .expect("retry policy should be valid");
    let mut budget = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("manual clock should represent the policy");
    let attempt = budget.begin_attempt().expect("first attempt is admitted");
    let _ = budget.finish_attempt(attempt).expect("valid completion");

    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetError::Exhausted(RetryBudgetExhausted::Attempts))
    ));
}

/// A sequential retry budget must not admit overlapping operations.
#[test]
fn test_begin_attempt_rejects_overlap() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .build()
        .expect("valid policy");
    let mut budget = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("valid budget");
    let _attempt = budget.begin_attempt().expect("first admission");
    assert!(budget.begin_attempt().is_err(), "overlap must be rejected");
}

/// Equal ordinals on the same clock do not make tokens interchangeable.
#[test]
fn test_finish_attempt_rejects_foreign_budget() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder().build().expect("valid policy");
    let mut first = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("first budget");
    let mut second = RetryBudget::new(&clock, *policy.admission_limits())
        .expect("second budget");
    let token = first.begin_attempt().expect("first admission");
    let other = second.begin_attempt().expect("second admission");
    let result = second.finish_attempt(token);
    assert!(matches!(result, Err(RetryBudgetError::InvalidAttempt)));
    assert_eq!(
        second
            .finish_attempt(other)
            .expect("foreign token did not corrupt state")
            .attempts(),
        1
    );
}
