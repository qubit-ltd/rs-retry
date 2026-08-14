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
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

#[test]
fn sync_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.sync();
}

struct ExhaustsBeforeSecondAttempt(Arc<ManualMonotonicClock>);

impl RetryObserver<TestError> for ExhaustsBeforeSecondAttempt {
    fn on_attempt_started(&self, context: &RetryContext) {
        if context.attempt() == 2 {
            self.0
                .advance(Duration::from_secs(1))
                .expect("manual clock should advance");
        }
    }
}

#[test]
fn sync_retry_preserves_last_failure_when_next_attempt_is_rejected() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .max_total_elapsed(Duration::from_secs(1))
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let attempts = AtomicUsize::new(0);
    let error = Retry::<TestError>::builder(policy)
        .observer(ExhaustsBeforeSecondAttempt(Arc::clone(&clock)))
        .build()
        .sync()
        .timer(Arc::new(clock.new_timer()))
        .run(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(TestError("first attempt failed"))
        })
        .unwrap_err();

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::TotalBudgetExhausted);
    assert_eq!(
        error.last_failure(),
        Some(&AttemptFailure::Error(TestError("first attempt failed")))
    );
}
