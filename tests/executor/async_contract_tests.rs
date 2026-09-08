// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use std::future;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_retry::BackoffPolicy;
use qubit_retry::RetryConfig;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::TokioRetry;

use crate::support::UnitTestError;

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_facade_retries_and_preserves_success() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let attempts = Arc::new(AtomicU32::new(0));
    let result = TokioRetry::new(&retry)
        .run({
            let attempts = Arc::clone(&attempts);
            move || {
                let attempts = Arc::clone(&attempts);
                async move {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err(UnitTestError)
                    } else {
                        Ok(7_u32)
                    }
                }
            }
        })
        .await
        .expect("second attempt succeeds");
    assert_eq!(*result.value(), 7);
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_async_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let error = TokioRetry::new(&retry)
        .hard_attempt_timeout(Duration::from_millis(1))
        .run(|| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Err::<(), _>(UnitTestError)
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        }
    ));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_shorter_flow_timeout_reports_flow_source() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let clock = ManualMonotonicClock::new_shared();
    let executor = TokioRetry::new(&retry)
        .hard_attempt_timeout(Duration::from_secs(30))
        .hard_flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer());
    let future = executor.run(future::pending::<Result<(), UnitTestError>>);
    tokio::pin!(future);

    let reached = tokio::select! {
        result = &mut future => {
            panic!("retry completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(reached.elapsed_since_origin(), Duration::from_secs(1));

    let error = future.await.expect_err("flow timeout should terminate retry");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Flow,
        }
    ));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_flow_timeout_caps_retry_sleep() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::from_secs(30)))
        .build()
        .unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let clock = ManualMonotonicClock::new_shared();
    let attempts = Arc::new(AtomicU32::new(0));
    let executor = TokioRetry::new(&retry)
        .hard_flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer());
    let future = executor.run({
        let attempts = Arc::clone(&attempts);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            future::ready(Err::<(), _>(UnitTestError))
        }
    });
    tokio::pin!(future);

    let reached = tokio::select! {
        result = &mut future => {
            panic!("retry completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(
        reached.elapsed_since_origin(),
        Duration::from_secs(1),
        "the flow deadline, not the full backoff, must drive the timer"
    );

    let error = future.await.expect_err("flow timeout should terminate retry");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
