/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
#![cfg(feature = "tokio")]

use std::time::Duration;

use qubit_retry::{AttemptFailure, AttemptTimeoutSource, Retry, RetryContext, RetryErrorReason};

use crate::support::TestError;

/// Verifies async operation panic still propagates through the current task.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
#[should_panic(expected = "async operation panic")]
async fn test_run_async_panic_propagates() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .build()
        .expect("retry should build");

    let _ = retry
        .run_async::<(), _, _>(|| async { panic!("async operation panic") })
        .await;
}

/// Verifies async attempt timeout becomes a retry failure.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_attempt_timeout_can_abort() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .abort_on_timeout()
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<(), TestError>(())
        })
        .await
        .expect_err("timeout should abort");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
    );
}

/// Verifies max elapsed caps an in-flight async attempt before a configured timeout.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_max_operation_elapsed_caps_in_flight_attempt_before_configured_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout(Some(Duration::from_millis(200)))
        .no_delay()
        .build()
        .expect("retry should build");

    let started = std::time::Instant::now();
    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok::<_, TestError>("late")
        })
        .await
        .expect_err("max elapsed should stop the in-flight async attempt");
    let elapsed = started.elapsed();

    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(20))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxOperationElapsed)
    );
    assert!(
        elapsed < Duration::from_millis(100),
        "max elapsed should stop before the configured timeout, elapsed: {elapsed:?}"
    );
}

/// Verifies max total elapsed caps an in-flight async attempt before a configured timeout.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_max_total_elapsed_caps_in_flight_attempt_before_configured_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout(Some(Duration::from_millis(200)))
        .no_delay()
        .build()
        .expect("retry should build");

    let started = std::time::Instant::now();
    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok::<_, TestError>("late")
        })
        .await
        .expect_err("max total elapsed should stop the in-flight async attempt");
    let elapsed = started.elapsed();

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert!(
        error.context().attempt_timeout() <= Some(Duration::from_millis(20)),
        "max total elapsed timeout should not exceed configured budget: {:?}",
        error.context().attempt_timeout()
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxTotalElapsed)
    );
    assert!(
        elapsed < Duration::from_millis(100),
        "max total elapsed should stop before the configured timeout, elapsed: {elapsed:?}"
    );
}

/// Verifies a shorter configured timeout still wins over remaining max elapsed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_configured_timeout_wins_when_shorter_than_max_operation_elapsed() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_millis(200)))
        .attempt_timeout(Some(Duration::from_millis(20)))
        .abort_on_timeout()
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok::<_, TestError>("late")
        })
        .await
        .expect_err("configured attempt timeout should abort first");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(20))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
    );
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies a configured timeout policy wins when it equals remaining max elapsed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_configured_timeout_policy_wins_when_equal_to_remaining_elapsed() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout(Some(Duration::from_millis(20)))
        .abort_on_timeout()
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok::<_, TestError>("late")
        })
        .await
        .expect_err("configured timeout policy should abort on equal timeout");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(20))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
    );
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies ordinary async failures can retry while max elapsed bounds attempts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_error_before_remaining_elapsed_timeout_can_retry() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(200)))
        .no_delay()
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let value = retry
        .run_async(|| {
            attempts += 1;
            async move {
                if attempts == 1 {
                    Err(TestError("transient"))
                } else {
                    Ok("done")
                }
            }
        })
        .await
        .expect("ordinary error should retry before remaining elapsed timeout");

    assert_eq!(value, "done");
    assert_eq!(attempts, 2);
}

/// Verifies async retry succeeds without per-attempt timeout after a retry delay.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when async retry does not reach success.
#[tokio::test(start_paused = true)]
async fn test_run_async_without_timeout_retries_until_success() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_millis(1))
        .build()
        .expect("retry should build");
    let mut attempts = 0;

    let value = retry
        .run_async(|| {
            attempts += 1;
            let current_attempt = attempts;
            async move {
                if current_attempt == 1 {
                    Err(TestError("temporary"))
                } else {
                    Ok("done")
                }
            }
        })
        .await
        .expect("second async attempt should succeed");

    assert_eq!(value, "done");
    assert_eq!(attempts, 2);
}

/// Verifies async timeout wrapping preserves fast successful results.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout wrapping changes success output.
#[tokio::test(start_paused = true)]
async fn test_run_async_with_attempt_timeout_allows_fast_success() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(10)))
        .no_delay()
        .build()
        .expect("retry should build");

    let value = retry
        .run_async(|| async { Ok::<_, TestError>("fast") })
        .await
        .expect("fast async attempt should succeed");

    assert_eq!(value, "fast");
}

/// Verifies async execution can stop before the first attempt on elapsed budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when async elapsed-budget handling differs.
#[tokio::test]
async fn test_run_async_max_operation_elapsed_can_stop_before_first_attempt() {
    let retry = Retry::<TestError>::builder()
        .max_operation_elapsed(Some(Duration::ZERO))
        .attempt_timeout(Some(Duration::from_millis(1)))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async::<(), _, _>(|| async { panic!("operation must not run") })
        .await
        .expect_err("zero elapsed budget should stop before first attempt");

    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.attempts(), 0);
    assert_eq!(error.context().attempt_timeout(), Some(Duration::ZERO));
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxOperationElapsed)
    );
}

/// Verifies async execution includes before-attempt listener time in max total elapsed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[tokio::test]
async fn test_run_async_max_total_elapsed_includes_before_attempt_listener_time() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .before_attempt(|_context: &RetryContext| {
            std::thread::sleep(Duration::from_millis(40));
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run_async::<(), _, _>(|| async { panic!("operation must not run") })
        .await
        .expect_err("before-attempt listener time should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert!(error.last_failure().is_none());
    assert!(error.context().total_elapsed() >= Duration::from_millis(20));
}

/// Verifies async retry handles zero retry delay without sleeping.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when zero-delay async retry does not proceed.
#[tokio::test]
async fn test_run_async_zero_delay_retry_skips_sleep() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .build()
        .expect("retry should build");
    let mut attempts = 0;

    let value = retry
        .run_async(|| {
            attempts += 1;
            let current_attempt = attempts;
            async move {
                if current_attempt == 1 {
                    Err(TestError("temporary"))
                } else {
                    Ok("done")
                }
            }
        })
        .await
        .expect("second async attempt should succeed");

    assert_eq!(value, "done");
    assert_eq!(attempts, 2);
}
