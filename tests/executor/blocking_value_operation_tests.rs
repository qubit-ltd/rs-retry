// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_retry::AttemptCancellationToken;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryPolicy;
use qubit_retry::WorkerRetry;

use crate::support::TestError;

/// Non-clone value used to verify worker value capture.
#[derive(Debug, PartialEq, Eq)]
struct NonCloneValue {
    /// Captured value text.
    text: &'static str,
}

/// Verifies blocking worker value capture through the public retry API.
#[test]
fn test_blocking_value_operation_is_observable_through_non_clone_success_value() {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("retry should build");
    let retry = RetryConfig::<TestError>::builder()
        .policy(policy)
        .build()
        .expect("valid config");

    let value = WorkerRetry::new(&retry)
        .run(|_token: AttemptCancellationToken| Ok::<_, TestError>(NonCloneValue { text: "ok" }))
        .expect("worker operation should succeed");

    assert_eq!(value.into_value_discarding_diagnostics(), NonCloneValue { text: "ok" });
}

/// Marks destruction of the discarded first worker result.
#[derive(Debug)]
struct DropTrackedValue {
    dropped: Arc<AtomicBool>,
    mark_on_drop: bool,
}

impl Drop for DropTrackedValue {
    fn drop(&mut self) {
        if self.mark_on_drop {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
}

#[test]
fn test_worker_discards_timed_out_value_before_retry() {
    let clock = ManualMonotonicClock::new_shared();
    let dropped = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_drop = Arc::clone(&dropped);
    let config = RetryConfig::<()>::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .observer(move |failure: &AttemptFailure<()>, _: &RetryContext| {
            if matches!(failure, AttemptFailure::TimedOut { .. }) {
                assert!(observed_drop.load(Ordering::SeqCst));
            }
        })
        .rule(|failure: &AttemptFailure<()>, _: &RetryContext| {
            if matches!(failure, AttemptFailure::TimedOut { .. }) {
                RetryDecision::Retry
            } else {
                RetryDecision::Abort
            }
        })
        .build()
        .expect("valid config");
    let operation_clock = Arc::clone(&clock);
    let operation_dropped = Arc::clone(&dropped);
    let operation_calls = Arc::clone(&calls);
    let result = WorkerRetry::new(&config)
        .timer(clock.new_timer())
        .hard_attempt_timeout(Duration::from_secs(1))
        .cancellation_grace(Duration::from_secs(5))
        .run(move |token: AttemptCancellationToken| {
            let first = operation_calls.fetch_add(1, Ordering::SeqCst) == 0;
            if first {
                operation_clock.advance(Duration::from_secs(1)).expect("expire attempt");
                let watchdog = Instant::now() + Duration::from_secs(5);
                while !token.is_cancelled() {
                    assert!(Instant::now() < watchdog, "worker cancellation watchdog");
                    thread::yield_now();
                }
            } else {
                assert!(operation_dropped.load(Ordering::SeqCst));
            }
            Ok::<_, ()>(DropTrackedValue {
                dropped: Arc::clone(&operation_dropped),
                mark_on_drop: first,
            })
        })
        .expect("second attempt succeeds after releasing the first value");
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(result.context().attempts(), 2);
}

#[test]
fn test_worker_keeps_success_value_owned_by_caller() {
    let dropped = Arc::new(AtomicBool::new(false));
    let operation_dropped = Arc::clone(&dropped);
    let config = RetryConfig::<()>::builder().build().expect("valid config");
    let success = WorkerRetry::new(&config)
        .run(move |_token: AttemptCancellationToken| {
            Ok::<_, ()>(DropTrackedValue {
                dropped: Arc::clone(&operation_dropped),
                mark_on_drop: true,
            })
        })
        .expect("worker operation should succeed");

    assert!(!dropped.load(Ordering::SeqCst));
    drop(success);
    assert!(dropped.load(Ordering::SeqCst));
}
