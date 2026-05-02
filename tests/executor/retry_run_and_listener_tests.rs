/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::panic;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qubit_error::BoxError;
use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, AttemptTimeoutSource, Retry, RetryContext, RetryError,
    RetryErrorReason,
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

/// Verifies the default boxed error type works through the retry executor.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when default error handling changes.
#[test]
fn test_run_default_boxed_error_type_exhausts_attempts() {
    let retry = Retry::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), BoxError> { Err(Box::new(TestError("boxed"))) })
        .expect_err("single boxed error should exhaust attempts");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(
        error
            .last_error()
            .expect("boxed error should be preserved")
            .to_string(),
        "boxed"
    );
}

/// Verifies the default boxed error type exercises listener and retry-delay paths.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when boxed-error retry behavior changes.
#[test]
fn test_run_default_boxed_error_type_observes_listeners_and_hints() {
    let before_attempts = Arc::new(Mutex::new(Vec::new()));
    let successes = Arc::new(Mutex::new(Vec::new()));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let retries = Arc::new(Mutex::new(Vec::new()));
    let terminal_errors = Arc::new(Mutex::new(Vec::new()));

    let before_events = Arc::clone(&before_attempts);
    let success_events = Arc::clone(&successes);
    let failure_events = Arc::clone(&failures);
    let retry_events = Arc::clone(&retries);
    let error_events = Arc::clone(&terminal_errors);
    let retry = Retry::<BoxError>::builder()
        .max_attempts(2)
        .no_delay()
        .retry_after_from_error(|error: &BoxError| {
            if error.to_string() == "hinted" {
                Some(Duration::ZERO)
            } else {
                None
            }
        })
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
        .on_failure(
            move |failure: &AttemptFailure<BoxError>, context: &RetryContext| {
                let message = failure
                    .as_error()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "timeout".to_string());
                failure_events
                    .lock()
                    .expect("failure events should be lockable")
                    .push((context.attempt(), context.retry_after_hint(), message));
                AttemptFailureDecision::UseDefault
            },
        )
        .on_retry(
            move |failure: &AttemptFailure<BoxError>, context: &RetryContext| {
                retry_events
                    .lock()
                    .expect("retry events should be lockable")
                    .push((
                        context.attempt(),
                        context.next_delay(),
                        failure
                            .as_error()
                            .map(ToString::to_string)
                            .expect("retry failure should wrap boxed error"),
                    ));
            },
        )
        .on_error(
            move |error: &RetryError<BoxError>, context: &RetryContext| {
                error_events
                    .lock()
                    .expect("terminal errors should be lockable")
                    .push((
                        error.reason(),
                        context.attempt(),
                        error
                            .last_error()
                            .map(ToString::to_string)
                            .expect("terminal boxed error should exist"),
                    ));
            },
        )
        .build()
        .expect("retry should build");

    let mut success_attempts = 0;
    let value = retry
        .run(|| -> Result<&'static str, BoxError> {
            success_attempts += 1;
            if success_attempts == 1 {
                Err(Box::new(TestError("hinted")))
            } else {
                Ok("done")
            }
        })
        .expect("second attempt should succeed");

    let mut failure_attempts = 0;
    let error = retry
        .run(|| -> Result<(), BoxError> {
            failure_attempts += 1;
            if failure_attempts == 1 {
                Err(Box::new(TestError("plain")))
            } else {
                Err(Box::new(TestError("terminal")))
            }
        })
        .expect_err("second run should exhaust attempts");

    assert_eq!(value, "done");
    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(
        *before_attempts
            .lock()
            .expect("before events should be lockable"),
        vec![1, 2, 1, 2]
    );
    assert_eq!(
        *successes.lock().expect("success events should be lockable"),
        vec![2]
    );
    assert_eq!(
        *failures.lock().expect("failure events should be lockable"),
        vec![
            (1, Some(Duration::ZERO), "hinted".to_string()),
            (1, None, "plain".to_string()),
            (2, None, "terminal".to_string()),
        ]
    );
    assert_eq!(
        *retries.lock().expect("retry events should be lockable"),
        vec![
            (1, Some(Duration::ZERO), "hinted".to_string()),
            (1, Some(Duration::ZERO), "plain".to_string()),
        ]
    );
    assert_eq!(
        *terminal_errors
            .lock()
            .expect("terminal errors should be lockable"),
        vec![(
            RetryErrorReason::AttemptsExceeded,
            2,
            "terminal".to_string()
        )]
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
    let scheduled = Arc::new(Mutex::new(Vec::new()));
    let scheduled_events = Arc::clone(&scheduled);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_secs(10))
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                AttemptFailureDecision::RetryAfter(Duration::from_millis(1))
            },
        )
        .on_retry(
            move |failure: &AttemptFailure<TestError>, context: &RetryContext| {
                scheduled_events
                    .lock()
                    .expect("retry scheduled events should be lockable")
                    .push((failure.as_error().cloned(), context.next_delay()));
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
    assert_eq!(
        *scheduled
            .lock()
            .expect("retry scheduled events should be lockable"),
        vec![(
            Some(TestError("still-failing")),
            Some(Duration::from_millis(1))
        )]
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

/// Verifies retry-after hint panics propagate by default.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_after_hint_panic_propagates_by_default() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .retry_after_hint(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| panic!("hint panic"),
        )
        .build()
        .expect("retry should build");

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let _ = retry.run(|| -> Result<(), TestError> { Err(TestError("failed")) });
    }));

    assert!(result.is_err());
}

/// Verifies listener panic isolation also isolates retry-after hint panics.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_after_hint_panic_is_isolated_when_enabled() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .retry_after_hint(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| panic!("hint panic"),
        )
        .isolate_listener_panics()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("isolated hint panic should fall back to retry failure handling");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(error.last_error(), Some(&TestError("failed")));
    assert_eq!(error.context().retry_after_hint(), None);
}

/// Verifies sync execution rejects configured attempt timeout.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_sync_run_with_attempt_timeout_is_unsupported() {
    let before_attempts = Arc::new(Mutex::new(Vec::new()));
    let on_error_contexts = Arc::new(Mutex::new(Vec::new()));
    let before_events = Arc::clone(&before_attempts);
    let on_error_events = Arc::clone(&on_error_contexts);
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .before_attempt(move |context: &RetryContext| {
            before_events
                .lock()
                .expect("before attempt events should be lockable")
                .push(context.attempt_timeout_source());
        })
        .on_error(
            move |error: &RetryError<TestError>, context: &RetryContext| {
                on_error_events
                    .lock()
                    .expect("error listener events should be lockable")
                    .push((
                        error.reason(),
                        context.attempt_timeout(),
                        context.attempt_timeout_source(),
                    ));
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("operation should fail");

    assert_eq!(error.reason(), RetryErrorReason::UnsupportedOperation);
    assert_eq!(error.context().attempt(), 0);
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
    );
    assert_eq!(error.context().retry_after_hint(), None);
    assert!(error.last_failure().is_none());
    assert_eq!(
        *before_attempts
            .lock()
            .expect("before attempt events should be lockable"),
        vec![]
    );
    assert_eq!(
        *on_error_contexts
            .lock()
            .expect("on_error events should be lockable"),
        vec![(
            RetryErrorReason::UnsupportedOperation,
            Some(Duration::from_millis(1)),
            Some(AttemptTimeoutSource::Configured)
        )]
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
fn test_max_operation_elapsed_can_stop_before_first_attempt() {
    let retry = Retry::<TestError>::builder()
        .max_operation_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");

    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.attempts(), 0);
    assert!(error.last_failure().is_none());
}

/// Verifies hook and retry sleep time do not count against elapsed budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_hook_and_retry_sleep_time_do_not_count_against_elapsed_budget() {
    let success_elapsed = Arc::new(Mutex::new(None));
    let success_elapsed_events = Arc::clone(&success_elapsed);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(10)))
        .fixed_delay(Duration::from_millis(25))
        .before_attempt(|_context: &RetryContext| {
            std::thread::sleep(Duration::from_millis(25));
        })
        .on_retry(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                std::thread::sleep(Duration::from_millis(25));
            },
        )
        .on_success(move |context: &RetryContext| {
            *success_elapsed_events
                .lock()
                .expect("success elapsed should be lockable") = Some(context.operation_elapsed());
        })
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let value = retry
        .run(|| {
            attempts += 1;
            if attempts == 1 {
                Err(TestError("retry-once"))
            } else {
                Ok("done")
            }
        })
        .expect("hook and retry sleep time should not exhaust elapsed budget");

    assert_eq!(value, "done");
    assert_eq!(attempts, 2);
    assert!(
        success_elapsed
            .lock()
            .expect("success elapsed should be lockable")
            .expect("success listener should run")
            < Duration::from_millis(10)
    );
}

/// Verifies retry listener time does not count against elapsed budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_on_retry_listener_time_does_not_count_against_elapsed_budget() {
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let scheduled_events = Arc::clone(&retry_events);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(10)))
        .fixed_delay(Duration::from_millis(25))
        .on_retry(
            move |failure: &AttemptFailure<TestError>, context: &RetryContext| {
                scheduled_events
                    .lock()
                    .expect("retry events should be lockable")
                    .push((failure.as_error().cloned(), context.next_delay()));
                std::thread::sleep(Duration::from_millis(25));
            },
        )
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let started = std::time::Instant::now();
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("slow-listener"))
        })
        .expect_err("attempts should be exhausted after listener and sleep time are excluded");
    let elapsed = started.elapsed();

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(error.attempts(), 2);
    assert_eq!(attempts, 2);
    assert_eq!(error.last_error(), Some(&TestError("slow-listener")));
    assert_eq!(error.context().next_delay(), None);
    assert_eq!(
        *retry_events
            .lock()
            .expect("retry events should be lockable"),
        vec![(
            Some(TestError("slow-listener")),
            Some(Duration::from_millis(25))
        )]
    );
    assert!(
        elapsed >= Duration::from_millis(50),
        "test should exercise retry listener and sleep wall time, elapsed: {elapsed:?}"
    );
}

/// Verifies total elapsed budget rejects a retry delay before sleeping it.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_rejects_retry_sleep_before_sleeping() {
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let scheduled_events = Arc::clone(&retry_events);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(50)))
        .fixed_delay(Duration::from_millis(200))
        .on_retry(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                scheduled_events
                    .lock()
                    .expect("retry events should be lockable")
                    .push(context.next_delay());
            },
        )
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let started = std::time::Instant::now();
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("retry-delay-too-large"))
        })
        .expect_err("total elapsed budget should reject the retry delay");
    let elapsed = started.elapsed();

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(
        error.last_error(),
        Some(&TestError("retry-delay-too-large"))
    );
    assert_eq!(
        error.context().next_delay(),
        Some(Duration::from_millis(200))
    );
    assert!(
        retry_events
            .lock()
            .expect("retry events should be lockable")
            .is_empty()
    );
    assert!(
        elapsed < Duration::from_millis(150),
        "retry delay should not be slept, elapsed: {elapsed:?}"
    );
}

/// Verifies retry-after delay participates in the total elapsed budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_rejects_retry_after_sleep_before_sleeping() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(50)))
        .no_delay()
        .retry_after_from_error(|_error: &TestError| Some(Duration::from_millis(200)))
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("retry-after-too-large"))
        })
        .expect_err("total elapsed budget should reject the retry-after delay");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(
        error.last_error(),
        Some(&TestError("retry-after-too-large"))
    );
    assert_eq!(
        error.context().retry_after_hint(),
        Some(Duration::from_millis(200))
    );
    assert_eq!(
        error.context().next_delay(),
        Some(Duration::from_millis(200))
    );
}

/// Verifies retry control-path listener time is included in total elapsed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_includes_failure_listener_time() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                std::thread::sleep(Duration::from_millis(40));
                AttemptFailureDecision::UseDefault
            },
        )
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("slow-listener"))
        })
        .expect_err("failure listener time should exhaust the total elapsed budget");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(error.last_error(), Some(&TestError("slow-listener")));
    assert!(error.context().total_elapsed() >= Duration::from_millis(20));
}

/// Verifies before-attempt listener time can exhaust total elapsed before operation runs.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_includes_before_attempt_listener_time() {
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
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("before-attempt listener time should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert!(error.last_failure().is_none());
    assert!(error.context().total_elapsed() >= Duration::from_millis(20));
}

/// Verifies on-retry listener time can exhaust total elapsed before retrying.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_includes_on_retry_listener_time() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .on_retry(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                std::thread::sleep(Duration::from_millis(40));
            },
        )
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("slow-on-retry"))
        })
        .expect_err("on-retry listener time should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(error.last_error(), Some(&TestError("slow-on-retry")));
    assert!(error.context().total_elapsed() >= Duration::from_millis(20));
}

/// Verifies on-retry listener time can leave too little total elapsed for the selected sleep.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_max_total_elapsed_rechecks_retry_sleep_after_on_retry_listener() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(80)))
        .fixed_delay(Duration::from_millis(50))
        .on_retry(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                std::thread::sleep(Duration::from_millis(40));
            },
        )
        .build()
        .expect("retry should build");

    let mut attempts = 0;
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("delay-after-slow-on-retry"))
        })
        .expect_err("on-retry listener time should make retry delay exceed total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(
        error.last_error(),
        Some(&TestError("delay-after-slow-on-retry"))
    );
    assert_eq!(
        error.context().next_delay(),
        Some(Duration::from_millis(50))
    );
}
