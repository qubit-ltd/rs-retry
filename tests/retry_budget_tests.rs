// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
// =============================================================================
//! Integration coverage for the public retry budget state machine.

mod budget;

use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryBudgetExhausted;
use qubit_retry::RetryPolicy;

/// Verifies an overrun is recorded exactly even though it prevents
/// continuation.
#[test]
fn test_finish_attempt_preserves_actual_overrun() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .max_operation_elapsed(Duration::from_secs(2))
        .max_total_elapsed(Duration::from_secs(10))
        .build()
        .expect("policy must be valid");
    let mut budget = RetryBudget::new(&clock, *policy.limits()).expect("budget must construct");

    let attempt = budget.begin_attempt().expect("first attempt must start");
    clock
        .advance(Duration::from_secs(3))
        .expect("clock must advance");
    let snapshot = budget.finish_attempt(attempt);

    assert_eq!(snapshot.attempts(), 1);
    assert_eq!(snapshot.attempt_elapsed(), Duration::from_secs(3));
    assert_eq!(snapshot.operation_elapsed(), Duration::from_secs(3));
    assert_eq!(
        budget.check_retry_after(Duration::ZERO),
        Err(RetryBudgetExhausted::OperationElapsed),
    );
}

/// Verifies simultaneous continuation limits report the stable priority order.
#[test]
fn test_begin_attempt_prioritizes_attempts_over_elapsed_limits() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .max_operation_elapsed(Duration::from_secs(1))
        .max_total_elapsed(Duration::from_secs(1))
        .build()
        .expect("policy must be valid");
    let mut budget = RetryBudget::new(&clock, *policy.limits()).expect("budget must construct");

    let attempt = budget.begin_attempt().expect("first attempt must start");
    clock
        .advance(Duration::from_secs(1))
        .expect("clock must advance");
    let _ = budget.finish_attempt(attempt);

    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetExhausted::Attempts)
    ));
}

/// Verifies a retry delay that reaches the deadline is rejected before sleep.
#[test]
fn test_check_retry_after_rejects_delay_at_deadline() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .max_total_elapsed(Duration::from_secs(1))
        .build()
        .expect("policy must be valid");
    let budget = RetryBudget::new(&clock, *policy.limits()).expect("budget must construct");

    assert_eq!(
        budget.check_retry_after(Duration::from_secs(1)),
        Err(RetryBudgetExhausted::TotalElapsed),
    );
}
