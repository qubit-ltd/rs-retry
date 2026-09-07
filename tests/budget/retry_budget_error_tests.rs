// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for retry budget construction errors.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ClockDomain;
use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::Timer;
use qubit_retry::RetryBudget;
use qubit_retry::RetryBudgetError;
use qubit_retry::RetryPolicy;

/// Soft budgets remain valid even when an absolute deadline would overflow.
#[test]
fn test_new_accepts_soft_budget_without_representable_deadline() {
    let clock = ManualMonotonicClock::new_shared();
    clock
        .advance(Duration::MAX)
        .expect("clock must reach its greatest instant");
    let policy = RetryPolicy::builder()
        .max_total_elapsed(Duration::from_nanos(1))
        .build()
        .expect("policy must be valid");

    let mut budget = RetryBudget::new(&clock, *policy.limits()).expect("soft budget needs no absolute deadline");
    let token = budget.begin_attempt().expect("initial operation remains admissible");
    assert_eq!(budget.finish_attempt(token).expect("valid clock").attempts(), 1);
}

/// A controllable clock permits contract violations without real-time races.
struct BrokenClock {
    origin: MonotonicInstant,
    nanos: AtomicU64,
}

impl MonotonicClock for BrokenClock {
    /// No timer is needed for direct budget accounting.
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("budget must not create a timer")
    }
    fn domain(&self) -> ClockDomain {
        self.origin.domain()
    }
    fn now(&self) -> MonotonicInstant {
        self.origin
            .checked_add(Duration::from_nanos(self.nanos.load(Ordering::Relaxed)))
            .expect("small test sample")
    }
}

/// Clock regressions are errors and cannot corrupt the latest valid accounting.
#[test]
fn test_clock_regression_returns_structured_error() {
    let clock = BrokenClock {
        origin: ManualMonotonicClock::new().now(),
        nanos: AtomicU64::new(10),
    };
    let policy = RetryPolicy::builder().build().expect("valid policy");
    let mut budget = RetryBudget::new(&clock, *policy.limits()).expect("valid domain");
    let attempt = budget.begin_attempt().expect("first attempt");
    clock.nanos.store(20, Ordering::Relaxed);
    let snapshot = budget.finish_attempt(attempt).expect("valid completion");
    assert_eq!(snapshot.operation_elapsed(), Duration::from_nanos(10));
    clock.nanos.store(15, Ordering::Relaxed);
    assert!(matches!(budget.snapshot(), Err(RetryBudgetError::Clock(_))));
    assert!(matches!(budget.begin_attempt(), Err(RetryBudgetError::Clock(_))));
    assert!(matches!(
        budget.check_retry_after(Duration::ZERO),
        Err(RetryBudgetError::Clock(_))
    ));
    clock.nanos.store(30, Ordering::Relaxed);
    let token = budget.begin_attempt().expect("valid admission after rejected sample");
    clock.nanos.store(25, Ordering::Relaxed);
    assert!(matches!(budget.finish_attempt(token), Err(RetryBudgetError::Clock(_))));
    clock.nanos.store(35, Ordering::Relaxed);
    let snapshot = budget.snapshot().expect("coherent observation");
    assert_eq!(snapshot.attempts(), 2);
    assert_eq!(snapshot.operation_elapsed(), Duration::from_nanos(10));
    assert!(matches!(
        budget.begin_attempt(),
        Err(RetryBudgetError::AttemptInProgress)
    ));
}
