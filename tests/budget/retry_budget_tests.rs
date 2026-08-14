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
use qubit_retry::RetryBudgetExhausted;
use qubit_retry::RetryPolicy;

#[test]
fn test_total_budget_rejects_delay_reaching_deadline() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_total_elapsed(Duration::from_secs(5))
        .build()
        .expect("retry policy should be valid");
    let budget = RetryBudget::new(&clock, *policy.limits())
        .expect("manual clock should represent the deadline");

    assert_eq!(
        budget.check_retry_after(Duration::from_secs(5)),
        Err(RetryBudgetExhausted::TotalElapsed)
    );
}

#[test]
fn test_operation_overrun_only_blocks_future_attempts() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .max_operation_elapsed(Duration::from_secs(1))
        .build()
        .expect("retry policy should be valid");
    let mut budget = RetryBudget::new(&clock, *policy.limits())
        .expect("manual clock should represent the policy");
    let attempt = budget.begin_attempt().expect("first attempt is admitted");
    clock
        .advance(Duration::from_secs(2))
        .expect("manual clock should advance");

    let snapshot = budget.finish_attempt(attempt);

    assert_eq!(snapshot.attempts(), 1);
    assert_eq!(snapshot.operation_elapsed(), Duration::from_secs(2));
    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetExhausted::OperationElapsed)
    ));
}

#[test]
fn test_attempt_budget_is_the_only_attempt_counter() {
    let clock = ManualMonotonicClock::new();
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .build()
        .expect("retry policy should be valid");
    let mut budget = RetryBudget::new(&clock, *policy.limits())
        .expect("manual clock should represent the policy");
    let attempt = budget.begin_attempt().expect("first attempt is admitted");
    let _ = budget.finish_attempt(attempt);

    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetExhausted::Attempts)
    ));
}
