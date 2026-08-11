// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Tests for retry budget snapshots.

use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryPolicy;

/// Verifies snapshots sample total time without changing admitted attempts.
#[test]
fn test_snapshot_samples_total_elapsed_without_mutation() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .build()
        .expect("policy must be valid");
    let budget = RetryBudget::new(&clock, *policy.limits()).expect("budget must construct");

    clock
        .advance(Duration::from_secs(2))
        .expect("clock must advance");
    let snapshot = budget.snapshot();

    assert_eq!(snapshot.attempts(), 0);
    assert_eq!(snapshot.total_elapsed(), Duration::from_secs(2));
    assert_eq!(snapshot.attempt_elapsed(), Duration::ZERO);
}
