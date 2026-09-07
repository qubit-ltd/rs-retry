// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryFailure;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;

use crate::support::UnitTestError;

#[test]
fn test_worker_facade_retries_with_cooperative_token() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<UnitTestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let result = retry
        .worker()
        .run({
            let attempts = Arc::clone(&attempts);
            move |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(UnitTestError)
                } else {
                    Ok(9_u32)
                }
            }
        })
        .expect("second worker attempt succeeds");
    assert_eq!(*result.value(), 9);
}

#[test]
fn test_worker_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<UnitTestError>::builder(policy).build();
    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let error = retry
        .worker()
        .timer(clock.new_timer())
        .attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::from_millis(50))
        .run(move |token| {
            operation_clock
                .advance(Duration::from_millis(1))
                .expect("expire admitted attempt");
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Err::<(), _>(UnitTestError)
        })
        .unwrap_err();

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Attempt
            }),
            ..
        }
    ));
}

#[test]
fn test_worker_shorter_flow_timeout_reports_flow_source() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<UnitTestError>::builder(policy).build();
    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let error = retry
        .worker()
        .timer(clock.new_timer())
        .attempt_timeout(Duration::from_secs(1))
        .flow_timeout(Duration::from_millis(10))
        .cancellation_grace(Duration::from_millis(50))
        .run(move |token| {
            operation_clock
                .advance(Duration::from_millis(10))
                .expect("expire admitted attempt");
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Err::<(), _>(UnitTestError)
        })
        .expect_err("flow timeout should terminate retry");

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Flow
            }),
            ..
        }
    ));
}

#[test]
fn test_worker_flow_timeout_caps_retry_sleep() {
    let clock = ManualMonotonicClock::new_shared();
    let worker_clock = Arc::clone(&clock);
    let attempts = Arc::new(AtomicU32::new(0));
    let operation_attempts = Arc::clone(&attempts);
    let cancellation = RetryCancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let (failed_sender, failed_receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let policy = RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_millis(500)))
            .build()
            .unwrap();
        Retry::<UnitTestError>::builder(policy)
            .observer(move |_: &AttemptFailure<UnitTestError>, _: &RetryContext| {
                failed_sender.send(()).expect("test controller alive");
            })
            .build()
            .worker()
            .timer(worker_clock.new_timer())
            .flow_timeout(Duration::from_millis(10))
            .cancellation_token(worker_cancellation)
            .run(move |_| {
                operation_attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(UnitTestError)
            })
            .map_err(Box::new)
    });
    let failed = failed_receiver.recv_timeout(Duration::from_secs(1)).is_ok();
    let deadline = if failed {
        clock.wait_for_next_deadline(Duration::from_secs(1))
    } else {
        None
    };
    if deadline.is_some() {
        clock.advance(Duration::from_millis(10)).expect("expire capped backoff");
    } else {
        cancellation.cancel();
    }
    let error = handle
        .join()
        .expect("worker controller completed")
        .expect_err("flow timeout");
    assert!(
        failed && deadline.is_some(),
        "backoff must register its capped deadline"
    );
    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.context().total_elapsed(), Duration::from_millis(10));
}
