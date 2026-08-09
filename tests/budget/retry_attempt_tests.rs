// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Tests for retry attempt tokens.

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryPolicy;

/// Verifies an admitted token exposes its one-based attempt ordinal.
#[test]
fn test_number_returns_admitted_attempt_ordinal() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .build()
        .expect("policy must be valid");
    let mut budget = RetryBudget::new(&clock, *policy.limits())
        .expect("budget must construct");

    let attempt = budget.begin_attempt().expect("attempt must start");

    assert_eq!(attempt.number(), 1);
}
