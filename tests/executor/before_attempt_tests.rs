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
use qubit_retry::Retry;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;

struct CancelBefore {
    token: RetryCancellationToken,
    callbacks: Arc<AtomicUsize>,
}

impl RetryObserver<&'static str> for CancelBefore {
    fn on_before_attempt(&self, context: &RetryContext) {
        assert_eq!(context.attempts(), 0);
        assert_eq!(context.current_attempt().expect("candidate").get(), 1);
        assert_eq!(context.current_hard_attempt_timeout(), None);
        self.callbacks.fetch_add(1, Ordering::SeqCst);
        self.token.cancel();
    }
}

#[test]
fn test_before_attempt_cancellation_does_not_admit_operation() {
    let token = RetryCancellationToken::new();
    let callbacks = Arc::new(AtomicUsize::new(0));
    let operations = AtomicUsize::new(0);
    let retry = RetryConfig::<&'static str>::builder()
        .observer(CancelBefore {
            token: token.clone(),
            callbacks: Arc::clone(&callbacks),
        })
        .build()
        .expect("valid config");
    let error = Retry::new(&retry)
        .cancellation_token(token)
        .run(|| {
            operations.fetch_add(1, Ordering::SeqCst);
            Ok::<(), &'static str>(())
        })
        .expect_err("before-attempt cancellation should stop the flow");
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(operations.load(Ordering::SeqCst), 0);
    assert_eq!(error.context().attempts(), 0);
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::BeforeAttempt,
            ..
        }
    ));
}

struct ExpireBefore {
    clock: Arc<ManualMonotonicClock>,
}

impl RetryObserver<&'static str> for ExpireBefore {
    fn on_before_attempt(&self, context: &RetryContext) {
        assert_eq!(context.attempts(), 0);
        self.clock
            .advance(Duration::from_secs(1))
            .expect("manual clock should advance");
    }
}

#[test]
fn test_before_attempt_elapsed_time_can_reject_candidate() {
    let clock = ManualMonotonicClock::new_shared();
    let retry = RetryConfig::<&'static str>::builder()
        .total_time_budget(Duration::from_secs(1))
        .observer(ExpireBefore {
            clock: Arc::clone(&clock),
        })
        .build()
        .expect("valid config");
    let error = Retry::new(&retry)
        .timer(clock.new_timer())
        .run(|| -> Result<(), &'static str> {
            panic!("candidate should not be admitted");
        })
        .expect_err("the candidate should be rejected");
    assert_eq!(error.context().attempts(), 0);
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));
}
