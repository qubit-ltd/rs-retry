// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Tests for retry budget construction errors.

use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryBudgetError;
use qubit_retry::RetryPolicy;

/// Verifies an unrepresentable total deadline returns a construction error.
#[test]
fn test_new_reports_clock_error_for_unrepresentable_deadline() {
    let clock = ManualMonotonicClock::new_shared();
    clock
        .advance(Duration::MAX)
        .expect("clock must reach its greatest instant");
    let policy = RetryPolicy::builder()
        .max_total_elapsed(Duration::from_nanos(1))
        .build()
        .expect("policy must be valid");

    let error = match RetryBudget::new(&clock, *policy.limits()) {
        Ok(_) => panic!("deadline overflow must fail construction"),
        Err(error) => error,
    };

    assert!(matches!(error, RetryBudgetError::Clock(_)));
}
