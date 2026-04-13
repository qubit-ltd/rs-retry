/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qubit_retry::{
    AttemptContext, AttemptFailure, Delay, RetryDecision, RetryError, RetryExecutor,
};

use crate::support::{NonCloneValue, TestError};

/// Verifies synchronous success does not require success values to be cloneable.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when success handling requires extra
/// traits or emits the wrong event count.
#[test]
fn test_run_returns_success_without_requiring_success_traits() {
    let success_events = Arc::new(AtomicUsize::new(0));
    let success_events_for_listener = Arc::clone(&success_events);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(3)
        .delay(Delay::none())
        .on_success(move |event| {
            assert_eq!(event.attempts, 1);
            success_events_for_listener.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("executor should be built");

    let value = executor
        .run(|| {
            Ok(NonCloneValue {
                value: "ready".to_string(),
            })
        })
        .expect("operation should succeed");

    assert_eq!(value.value, "ready");
    assert_eq!(success_events.load(Ordering::SeqCst), 1);
}

/// Verifies synchronous retry sleeps on non-zero delays and honors retry_if.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the retry branch does not reach
/// success after the configured delay.
#[test]
fn test_run_uses_nonzero_sleep_and_retry_if_retry_branch() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .delay(Delay::fixed(Duration::from_millis(1)))
        .retry_if(|_: &TestError, _: &AttemptContext| true)
        .build()
        .expect("executor should be built");

    let result = executor.run(|| {
        let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == 1 {
            Err(TestError("sleep-before-retry"))
        } else {
            Ok("slept")
        }
    });

    assert_eq!(result.expect("retry should eventually succeed"), "slept");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies synchronous retry continues until success and emits retry events.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when attempts or retry event payloads are
/// incorrect.
#[test]
fn test_run_retries_until_success_and_emits_retry_events() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let retry_events_for_listener = Arc::clone(&retry_events);
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(4)
        .delay(Delay::none())
        .on_retry(move |event| {
            if let AttemptFailure::Error(error) = event.failure {
                retry_events_for_listener
                    .lock()
                    .expect("retry event list should be lockable")
                    .push((event.attempt, event.max_attempts, event.next_delay, error.0));
            }
        })
        .build()
        .expect("executor should be built");

    let result = executor.run(|| {
        let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt < 3 {
            Err(TestError("transient"))
        } else {
            Ok("done")
        }
    });

    assert_eq!(result.expect("retry should eventually succeed"), "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(
        *retry_events
            .lock()
            .expect("retry event list should be lockable"),
        vec![
            (1, 4, Duration::ZERO, "transient"),
            (2, 4, Duration::ZERO, "transient")
        ]
    );
}

/// Verifies retry_if can abort and preserve the original application error.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when abort events or preserved errors are
/// incorrect.
#[test]
fn test_retry_if_can_abort_and_preserve_original_error() {
    let abort_events = Arc::new(AtomicUsize::new(0));
    let abort_events_for_listener = Arc::clone(&abort_events);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(3)
        .delay(Delay::none())
        .retry_if(|error: &TestError, _: &AttemptContext| error.0 == "retry")
        .on_abort(move |event| {
            assert_eq!(event.attempts, 1);
            assert!(matches!(
                event.failure,
                AttemptFailure::Error(TestError("fatal"))
            ));
            abort_events_for_listener.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("executor should be built");

    let error = executor
        .run(|| -> Result<(), TestError> { Err(TestError("fatal")) })
        .expect_err("fatal error should abort retries");

    assert_eq!(abort_events.load(Ordering::SeqCst), 1);
    assert_eq!(error.attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("fatal")));
    assert_eq!(error.into_last_error(), Some(TestError("fatal")));
}

/// Verifies classifiers can inspect attempt context before deciding.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when context values or abort decisions are
/// incorrect.
#[test]
fn test_classify_error_can_use_attempt_context() {
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(4)
        .delay(Delay::none())
        .classify_error(|_, context| {
            assert!(context.attempt <= context.max_attempts);
            if context.attempt < 2 {
                RetryDecision::Retry
            } else {
                RetryDecision::Abort
            }
        })
        .build()
        .expect("executor should be built");

    let error = executor
        .run(|| -> Result<(), TestError> { Err(TestError("still-bad")) })
        .expect_err("classifier should abort on second attempt");

    assert!(matches!(error, RetryError::Aborted { attempts: 2, .. }));
}

/// Verifies attempt exhaustion emits a failure event and preserves last error.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when exhaustion metadata or failure events
/// are incorrect.
#[test]
fn test_attempts_exceeded_emits_failure_event_and_preserves_last_error() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let failures = Arc::new(Mutex::new(Vec::new()));
    let failures_for_listener = Arc::clone(&failures);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .delay(Delay::none())
        .on_failure(move |event| {
            let failure = match event.failure {
                Some(AttemptFailure::Error(error)) => error.0,
                _ => "missing",
            };
            failures_for_listener
                .lock()
                .expect("failure event list should be lockable")
                .push((event.attempts, failure));
        })
        .build()
        .expect("executor should be built");

    let error = executor
        .run(|| -> Result<(), TestError> {
            let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt == 1 {
                Err(TestError("first"))
            } else {
                Err(TestError("second"))
            }
        })
        .expect_err("attempt limit should be exceeded");

    assert!(matches!(
        error,
        RetryError::AttemptsExceeded {
            attempts: 2,
            max_attempts: 2,
            ..
        }
    ));
    assert_eq!(error.last_error(), Some(&TestError("second")));
    assert_eq!(
        *failures
            .lock()
            .expect("failure event list should be lockable"),
        vec![(2, "second")]
    );
}

/// Verifies zero elapsed budget can stop before the first attempt.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the operation runs or failure
/// metadata is incorrect.
#[test]
fn test_max_elapsed_can_stop_before_first_attempt() {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let failures_for_listener = Arc::clone(&failures);
    let executor = RetryExecutor::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .on_failure(move |event| {
            failures_for_listener
                .lock()
                .expect("failure event list should be lockable")
                .push((event.attempts, event.failure.is_none()));
        })
        .build()
        .expect("executor should be built");

    let error = executor
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("elapsed budget should stop before first attempt");

    assert!(matches!(
        error,
        RetryError::MaxElapsedExceeded {
            attempts: 0,
            last_failure: None,
            ..
        }
    ));
    assert_eq!(
        *failures
            .lock()
            .expect("failure event list should be lockable"),
        vec![(0, true)]
    );
}

/// Verifies elapsed budget stops when the next retry sleep would exceed it.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when retry events are emitted or the wrong
/// terminal error is returned.
#[test]
fn test_max_elapsed_stops_when_retry_sleep_would_exceed_budget() {
    let retry_events = Arc::new(AtomicUsize::new(0));
    let retry_events_for_listener = Arc::clone(&retry_events);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(3)
        .max_elapsed(Some(Duration::from_millis(1)))
        .delay(Delay::fixed(Duration::from_millis(10)))
        .on_retry(move |_| {
            retry_events_for_listener.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("executor should be built");

    let error = executor
        .run(|| -> Result<(), TestError> { Err(TestError("late")) })
        .expect_err("elapsed budget should reject retry sleep");

    assert_eq!(retry_events.load(Ordering::SeqCst), 0);
    assert!(matches!(
        error,
        RetryError::MaxElapsedExceeded {
            attempts: 1,
            last_failure: Some(AttemptFailure::Error(TestError("late"))),
            ..
        }
    ));
}

/// Verifies elapsed budget allows retry sleeps that remain within the budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when a retry inside the elapsed budget is
/// rejected or the retry event is missing.
#[test]
fn test_max_elapsed_allows_retry_sleep_within_budget() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let retry_events = Arc::new(AtomicUsize::new(0));
    let retry_events_for_listener = Arc::clone(&retry_events);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .max_elapsed(Some(Duration::from_millis(100)))
        .delay(Delay::fixed(Duration::from_millis(1)))
        .on_retry(move |_| {
            retry_events_for_listener.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("executor should be built");

    let result = executor
        .run(|| {
            let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt == 1 {
                Err(TestError("within-budget"))
            } else {
                Ok("retried")
            }
        })
        .expect("retry inside elapsed budget should succeed");

    assert_eq!(result, "retried");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(retry_events.load(Ordering::SeqCst), 1);
}

/// Verifies asynchronous retry continues until success.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when async retry attempts or returned
/// values are incorrect.
#[tokio::test]
async fn test_run_async_retries_until_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(3)
        .delay(Delay::none())
        .build()
        .expect("executor should be built");

    let result = executor
        .run_async(|| {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 1 {
                    Err(TestError("retry"))
                } else {
                    Ok(NonCloneValue {
                        value: "async-ready".to_string(),
                    })
                }
            }
        })
        .await
        .expect("async retry should eventually succeed");

    assert_eq!(result.value, "async-ready");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies zero elapsed budget stops async execution before the first attempt.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the async operation runs or the wrong
/// terminal error is returned.
#[tokio::test]
async fn test_run_async_can_stop_before_first_attempt() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .build()
        .expect("executor should be built");

    let error = executor
        .run_async(|| {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                attempts_for_operation.fetch_add(1, Ordering::SeqCst);
                Ok::<_, TestError>("must-not-run")
            }
        })
        .await
        .expect_err("elapsed budget should stop before first async attempt");

    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    assert!(matches!(
        error,
        RetryError::MaxElapsedExceeded { attempts: 0, .. }
    ));
}

/// Verifies async retry sleeps on non-zero delays between attempts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when async retry does not eventually
/// succeed after the configured delay.
#[tokio::test]
async fn test_run_async_uses_nonzero_sleep_between_attempts() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .delay(Delay::fixed(Duration::from_millis(1)))
        .build()
        .expect("executor should be built");

    let result = executor
        .run_async(|| {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 1 {
                    Err(TestError("async-sleep"))
                } else {
                    Ok("async-slept")
                }
            }
        })
        .await
        .expect("async retry should eventually succeed");

    assert_eq!(result, "async-slept");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies async execution returns a terminal error from a failed attempt.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when attempt exhaustion metadata is
/// incorrect.
#[tokio::test]
async fn test_run_async_returns_finished_error_from_failed_attempt() {
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(1)
        .delay(Delay::none())
        .build()
        .expect("executor should be built");

    let error = executor
        .run_async(|| async { Err::<(), TestError>(TestError("async-exhausted")) })
        .await
        .expect_err("single failed attempt should exhaust retries");

    assert!(matches!(
        error,
        RetryError::AttemptsExceeded {
            attempts: 1,
            max_attempts: 1,
            last_failure: AttemptFailure::Error(TestError("async-exhausted")),
            ..
        }
    ));
}

/// Verifies timed async attempts can retry after timeout and then succeed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout retry events or attempt
/// counts are incorrect.
#[tokio::test]
async fn test_run_async_with_timeout_can_retry_and_succeed() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let retry_timeouts = Arc::new(AtomicUsize::new(0));
    let retry_timeouts_for_listener = Arc::clone(&retry_timeouts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .delay(Delay::fixed(Duration::from_millis(1)))
        .on_retry(move |event| {
            if matches!(event.failure, AttemptFailure::AttemptTimeout { .. }) {
                retry_timeouts_for_listener.fetch_add(1, Ordering::SeqCst);
            }
        })
        .build()
        .expect("executor should be built");

    let result = executor
        .run_async_with_timeout(Duration::from_millis(5), || {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 1 {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Ok("too-late")
                } else {
                    Ok("ok")
                }
            }
        })
        .await
        .expect("timeout retry should eventually succeed");

    assert_eq!(result, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(retry_timeouts.load(Ordering::SeqCst), 1);
}

/// Verifies timed async attempts retry operation errors with sleep.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when operation errors are not retried
/// under the per-attempt timeout wrapper.
#[tokio::test]
async fn test_run_async_with_timeout_can_retry_operation_errors_with_sleep() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(2)
        .delay(Delay::fixed(Duration::from_millis(1)))
        .build()
        .expect("executor should be built");

    let result = executor
        .run_async_with_timeout(Duration::from_millis(50), || {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                let attempt = attempts_for_operation.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 1 {
                    Err(TestError("retry-error"))
                } else {
                    Ok("timeout-error-retried")
                }
            }
        })
        .await
        .expect("operation error should be retried before timeout");

    assert_eq!(result, "timeout-error-retried");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies timed async execution preserves operation errors on exhaustion.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the terminal error does not preserve
/// the operation error.
#[tokio::test]
async fn test_run_async_with_timeout_returns_finished_error_from_operation_error() {
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(1)
        .delay(Delay::none())
        .build()
        .expect("executor should be built");

    let error = executor
        .run_async_with_timeout(Duration::from_millis(50), || async {
            Err::<(), TestError>(TestError("timeout-operation-error"))
        })
        .await
        .expect_err("single operation error should exhaust retries");

    assert!(matches!(
        error,
        RetryError::AttemptsExceeded {
            attempts: 1,
            max_attempts: 1,
            last_failure: AttemptFailure::Error(TestError("timeout-operation-error")),
            ..
        }
    ));
}

/// Verifies zero elapsed budget stops timed async execution before attempts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the timed operation runs or the wrong
/// terminal error is returned.
#[tokio::test]
async fn test_run_async_with_timeout_can_stop_before_first_attempt() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_operation = Arc::clone(&attempts);
    let executor = RetryExecutor::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .build()
        .expect("executor should be built");

    let error = executor
        .run_async_with_timeout(Duration::from_millis(50), || {
            let attempts_for_operation = Arc::clone(&attempts_for_operation);
            async move {
                attempts_for_operation.fetch_add(1, Ordering::SeqCst);
                Ok::<_, TestError>("must-not-run")
            }
        })
        .await
        .expect_err("elapsed budget should stop before first timed attempt");

    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    assert!(matches!(
        error,
        RetryError::MaxElapsedExceeded { attempts: 0, .. }
    ));
}

/// Verifies timed async exhaustion returns timeout failure metadata.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout exhaustion does not return an
/// attempt-timeout failure.
#[tokio::test]
async fn test_run_async_with_timeout_exhaustion_returns_timeout_failure() {
    let executor = RetryExecutor::<TestError>::builder()
        .max_attempts(1)
        .delay(Delay::none())
        .build()
        .expect("executor should be built");

    let error = executor
        .run_async_with_timeout(Duration::from_millis(2), || async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<(), TestError>(())
        })
        .await
        .expect_err("timed out attempt should exhaust retries");

    assert!(matches!(
        error,
        RetryError::AttemptsExceeded {
            attempts: 1,
            max_attempts: 1,
            last_failure: AttemptFailure::AttemptTimeout { .. },
            ..
        }
    ));
    assert_eq!(error.into_last_error(), None);
}
