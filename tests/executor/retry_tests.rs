/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::sync::{Arc, Mutex};
use std::time::Duration;

use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryError, RetryErrorReason,
};

use crate::support::{NonCloneValue, TestError};

/// Verifies sync retry succeeds and emits attempt lifecycle events.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_retries_until_success_and_emits_attempt_events() {
    let before_attempts = Arc::new(Mutex::new(Vec::new()));
    let successes = Arc::new(Mutex::new(Vec::new()));
    let before_events = Arc::clone(&before_attempts);
    let success_events = Arc::clone(&successes);
    let mut attempts = 0;
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .before_attempt(move |context: &RetryContext| {
            before_events
                .lock()
                .expect("before events should be lockable")
                .push(context.attempt());
        })
        .on_success(move |context: &RetryContext| {
            success_events
                .lock()
                .expect("success events should be lockable")
                .push(context.attempt());
        })
        .build()
        .expect("retry should build");

    let value = retry
        .run(|| {
            attempts += 1;
            if attempts < 3 {
                Err(TestError("temporary"))
            } else {
                Ok(NonCloneValue {
                    value: "done".to_string(),
                })
            }
        })
        .expect("retry should eventually succeed");

    assert_eq!(value.value, "done");
    assert_eq!(
        *before_attempts
            .lock()
            .expect("before events should be lockable"),
        vec![1, 2, 3]
    );
    assert_eq!(
        *successes.lock().expect("success events should be lockable"),
        vec![3]
    );
}

/// Verifies a failure listener can abort retrying.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_on_failure_can_abort_retry_flow() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .on_failure(
            |failure: &AttemptFailure<TestError>, _context: &RetryContext| match failure {
                AttemptFailure::Error(TestError("fatal")) => AttemptFailureDecision::Abort,
                _ => AttemptFailureDecision::UseDefault,
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("fatal")) })
        .expect_err("fatal error should abort");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(error.attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("fatal")));
}

/// Verifies retry-after decisions override the configured delay.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_after_decision_selects_next_delay() {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let failure_events = Arc::clone(&failures);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_secs(10))
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                AttemptFailureDecision::RetryAfter(Duration::from_millis(1))
            },
        )
        .on_error(
            move |error: &RetryError<TestError>, context: &RetryContext| {
                failure_events
                    .lock()
                    .expect("failure events should be lockable")
                    .push((error.reason(), context.next_delay()));
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("still-failing")) })
        .expect_err("operation should fail after attempts are exhausted");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(
        *failures.lock().expect("failure events should be lockable"),
        vec![(RetryErrorReason::AttemptsExceeded, None)]
    );
}

/// Verifies retry-after hints can drive the default decision delay.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_after_hint_is_available_to_failure_listener() {
    let hints = Arc::new(Mutex::new(Vec::new()));
    let hint_events = Arc::clone(&hints);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .retry_after_from_error(|error| {
            if error.0 == "limited" {
                Some(Duration::from_millis(1))
            } else {
                None
            }
        })
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                hint_events
                    .lock()
                    .expect("hint events should be lockable")
                    .push(context.retry_after_hint());
                AttemptFailureDecision::UseDefault
            },
        )
        .build()
        .expect("retry should build");

    let _ = retry.run(|| -> Result<(), TestError> { Err(TestError("limited")) });

    assert_eq!(
        *hints.lock().expect("hint events should be lockable"),
        vec![
            Some(Duration::from_millis(1)),
            Some(Duration::from_millis(1))
        ]
    );
}

/// Verifies sync execution does not expose async-only attempt timeout metadata.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_sync_run_does_not_report_attempt_timeout() {
    let timeouts = Arc::new(Mutex::new(Vec::new()));
    let timeout_events = Arc::clone(&timeouts);
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                timeout_events
                    .lock()
                    .expect("timeout events should be lockable")
                    .push(context.attempt_timeout());
                AttemptFailureDecision::UseDefault
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("operation should fail");

    assert_eq!(error.context().attempt_timeout(), None);
    assert_eq!(
        *timeouts.lock().expect("timeout events should be lockable"),
        vec![None]
    );
}

/// Verifies async attempt timeout becomes a retry failure.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[cfg(feature = "tokio")]
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
}

/// Verifies elapsed budget can stop before the first attempt.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_elapsed_can_stop_before_first_attempt() {
    let retry = Retry::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");

    assert_eq!(error.reason(), RetryErrorReason::MaxElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert!(error.last_failure().is_none());
}
