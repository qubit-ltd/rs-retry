// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

#[derive(Default)]
struct LifecycleCounts {
    started: AtomicU32,
    failed: AtomicU32,
    scheduled: AtomicU32,
}

struct RecordingObserver(Arc<LifecycleCounts>);

impl RetryObserver<TestError> for RecordingObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        self.0.started.fetch_add(1, Ordering::SeqCst);
    }

    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        self.0.failed.fetch_add(1, Ordering::SeqCst);
    }

    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.0.scheduled.fetch_add(1, Ordering::SeqCst);
    }
}

struct PanickingObserver;

impl RetryObserver<TestError> for PanickingObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        panic!("observer panic");
    }
}

#[test]
fn test_observers_and_rules_cover_current_lifecycle() {
    let counts = Arc::new(LifecycleCounts::default());
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let config = RetryConfig::<TestError>::builder()
        .policy(policy.clone())
        .observer(PanickingObserver)
        .observer(RecordingObserver(Arc::clone(&counts)))
        .build()
        .expect("valid config");
    let observer_error = Retry::new(&config)
        .run(|| Ok::<_, TestError>(11_u32))
        .expect_err("the first started observer panic must terminate the flow");
    let RetryErrorReason::CallbackFailed { callback } = observer_error.reason()
    else {
        panic!("expected an observer callback failure");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), RetryCallbackPhase::BeforeAttempt);
    assert_eq!(observer_error.last_failure(), None);
    assert_eq!(observer_error.context().attempts(), 0);
    assert_eq!(
        observer_error
            .context()
            .current_attempt()
            .map(NonZeroU32::get),
        Some(1)
    );
    assert_eq!(counts.started.load(Ordering::SeqCst), 0);

    let config2 = RetryConfig::<TestError>::builder()
        .policy(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::UseDefault
        })
        .observer(RecordingObserver(Arc::clone(&counts)))
        .build()
        .expect("valid config");
    let rule_error = Retry::new(&config2)
        .run(|| Err::<u32, _>(TestError("retry")))
        .expect_err("the first rule panic must terminate the flow");
    let RetryErrorReason::CallbackFailed { callback } = rule_error.reason()
    else {
        panic!("expected a rule callback failure");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Rule);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), RetryCallbackPhase::RuleDecision);
    assert_eq!(
        rule_error.last_failure(),
        Some(&AttemptFailure::Error(TestError("retry")))
    );
    assert_eq!(counts.failed.load(Ordering::SeqCst), 1);
    assert_eq!(counts.scheduled.load(Ordering::SeqCst), 0);
    assert_eq!(
        rule_error.context().current_attempt().map(NonZeroU32::get),
        Some(1)
    );
}
