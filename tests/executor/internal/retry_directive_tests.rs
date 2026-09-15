// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-directive scheduling behavior.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRandomSource;

use crate::support::TestError;

type RetryHintRecord = Option<(Option<Duration>, Option<Duration>)>;

struct HintRecordingObserver(Arc<Mutex<RetryHintRecord>>);

impl RetryObserver<TestError> for HintRecordingObserver {
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        context: &RetryContext,
    ) {
        *self.0.lock().unwrap() =
            Some((context.retry_after_hint(), context.next_delay()));
    }
}

/// Verifies a retry directive exposes its hint and resolved schedule overlay.
#[test]
fn test_retry_directive_records_retry_hint_and_resolved_delay() {
    let recorded = Arc::new(Mutex::new(None));
    let attempts = AtomicU32::new(0);
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::ZERO).ignore_retry_after())
        .build()
        .unwrap();
    let config = RetryConfig::<TestError>::builder()
        .policy(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithHint(Duration::from_secs(3))
        })
        .observer(HintRecordingObserver(Arc::clone(&recorded)))
        .build()
        .expect("valid config");
    let result = Retry::new(&config)
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(())
            }
        })
        .expect("hinted retry should succeed");
    assert_eq!(
        *recorded.lock().unwrap(),
        Some((Some(Duration::from_secs(3)), Some(Duration::ZERO))),
    );
    assert_eq!(result.context().retry_after_hint(), None);
}

/// A selected delay is not scheduled when no retry can be admitted.
#[test]
fn test_retry_scheduled_is_not_emitted_after_attempt_exhaustion() {
    let recorded = Arc::new(Mutex::new(None));
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .build()
        .expect("valid policy");
    let config2 = RetryConfig::<TestError>::builder()
        .policy(policy)
        .observer(HintRecordingObserver(Arc::clone(&recorded)))
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let error = Retry::new(&config2)
        .run(|| Err::<(), _>(TestError("last")))
        .expect_err("one failed attempt exhausts the flow");
    assert_eq!(*recorded.lock().expect("record lock"), None);
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));
}

/// Scheduling callbacks cannot override an already exhausted terminal cause.
#[test]
fn test_retry_scheduled_panic_cannot_mask_exhaustion() {
    for policy in [
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("attempt policy"),
        RetryPolicy::builder()
            .total_time_budget(Duration::from_secs(1))
            .backoff(BackoffPolicy::fixed(Duration::from_secs(1)))
            .build()
            .expect("elapsed policy"),
    ] {
        let config3 = RetryConfig::<TestError>::builder()
            .policy(policy)
            .observer(crate::support::PanickingPhaseObserver::new(
                RetryCallbackPhase::RetryScheduled,
            ))
            .fallback(RetryFallback::Retry)
            .build()
            .expect("valid config");
        let error = Retry::new(&config3)
            .run(|| Err::<(), _>(TestError("retained")))
            .expect_err("budget exhausted");
        assert!(matches!(error.reason(), RetryErrorReason::Exhausted { .. }));
        assert_eq!(error.last_error().expect("retained error").0, "retained");
    }
}

/// Advances elapsed time while resolving jitter to expose stale budget samples.
struct AdvancingRandom(Arc<ManualMonotonicClock>);

impl RetryRandomSource for AdvancingRandom {
    fn random_f64_inclusive(&self, min: f64, _max: f64) -> f64 {
        self.0
            .advance(Duration::from_secs(2))
            .expect("advance clock");
        min
    }
}

/// Resolving jitter can consume the remaining budget before notification.
#[test]
fn test_retry_scheduled_rechecks_time_after_resolving_jitter() {
    let clock = ManualMonotonicClock::new_shared();
    let recorded = Arc::new(Mutex::new(None));
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .total_time_budget(Duration::from_secs(1))
        .backoff(
            BackoffPolicy::fixed(Duration::from_millis(1)).with_full_jitter(),
        )
        .build()
        .expect("valid policy");
    let config4 = RetryConfig::<TestError>::builder()
        .policy(policy)
        .observer(HintRecordingObserver(Arc::clone(&recorded)))
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let error = Retry::new(&config4)
        .timer(clock.new_timer())
        .random_source(Arc::new(AdvancingRandom(clock)))
        .run(|| Err::<(), _>(TestError("retained")))
        .expect_err("budget expired during jitter");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));
    assert_eq!(*recorded.lock().expect("record lock"), None);
}
