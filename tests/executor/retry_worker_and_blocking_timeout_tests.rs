/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken, AttemptFailure, AttemptFailureDecision, AttemptTimeoutOption,
    AttemptTimeoutPolicy, Retry, RetryContext, RetryErrorReason,
};

use crate::support::TestError;

/// Verifies worker execution uses a separate thread without timeout settings.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_executes_on_worker_without_timeout() {
    let main_thread = std::thread::current().id();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let worker_thread = retry
        .run_in_worker(move |token: AttemptCancelToken| {
            assert!(!token.is_cancelled());
            Ok::<_, TestError>(std::thread::current().id())
        })
        .expect("worker attempt should succeed");

    assert_ne!(worker_thread, main_thread);
}

/// Verifies worker panics become retry failures and abort by default.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_panic_aborts_by_default() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| -> Result<(), TestError> {
                attempts.fetch_add(1, Ordering::SeqCst);
                panic!("worker failed");
            }
        })
        .expect_err("worker panic should abort by default");

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    let panic = error
        .last_failure()
        .and_then(AttemptFailure::as_panic)
        .expect("terminal failure should be a captured panic");
    assert_eq!(panic.message(), "worker failed");
}

/// Verifies non-string worker panic payloads use the documented fallback text.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_non_string_panic_uses_fallback_message() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| -> Result<(), TestError> {
            std::panic::panic_any(123_u32);
        })
        .expect_err("non-string worker panic should abort");

    let panic = error
        .last_failure()
        .and_then(AttemptFailure::as_panic)
        .expect("terminal failure should be a captured panic");
    assert_eq!(
        panic.message(),
        "attempt panicked with a non-string payload"
    );
}

/// Verifies failure listeners can retry captured worker panics.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_panic_can_be_retried_by_listener() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .on_failure(
            |failure: &AttemptFailure<TestError>, _context: &RetryContext| match failure {
                AttemptFailure::Panic(panic) if panic.message() == "transient panic" => {
                    AttemptFailureDecision::Retry
                }
                _ => AttemptFailureDecision::UseDefault,
            },
        )
        .build()
        .expect("retry should build");

    let value = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    panic!("transient panic");
                }
                Ok::<_, TestError>("done")
            }
        })
        .expect("second worker attempt should succeed");

    assert_eq!(value, "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies blocking timeout aborts and signals the cooperative cancel token.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_blocking_with_timeout_can_abort_and_cancel_token() {
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(Duration::from_millis(5))))
        .build()
        .expect("retry should build");

    let error = retry
        .run_blocking_with_timeout({
            let saw_cancel = Arc::clone(&saw_cancel);
            move |token: AttemptCancelToken| {
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                saw_cancel.store(true, Ordering::SeqCst);
                Err::<(), TestError>(TestError("cancelled"))
            }
        })
        .expect_err("timeout should abort");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_millis(5))
    );
    for _ in 0..50 {
        if saw_cancel.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(saw_cancel.load(Ordering::SeqCst));
}

/// Verifies blocking timeout can retry and later return a successful result.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_blocking_with_timeout_retries_timeout_until_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::new(
            Duration::from_millis(50),
            AttemptTimeoutPolicy::Retry,
        )))
        .build()
        .expect("retry should build");

    let value = retry
        .run_blocking_with_timeout({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    thread::sleep(Duration::from_millis(200));
                    Ok::<_, TestError>("late")
                } else {
                    Ok::<_, TestError>("done")
                }
            }
        })
        .expect("second blocking attempt should succeed");

    assert_eq!(value, "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies worker mode honors max elapsed before running the first attempt.
#[test]
fn test_run_in_worker_max_elapsed_can_stop_before_first_attempt() {
    let calls = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let calls = Arc::clone(&calls);
            move |_token: AttemptCancelToken| -> Result<(), TestError> {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .expect_err("zero elapsed budget should stop before first attempt");

    assert_eq!(error.reason(), RetryErrorReason::MaxElapsedExceeded);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// Verifies worker mode sleeps when retrying with non-zero delay.
#[test]
fn test_run_in_worker_retries_with_non_zero_delay() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_millis(2))
        .build()
        .expect("retry should build");
    let start = std::time::Instant::now();

    let value = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| -> Result<&'static str, TestError> {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 1 {
                    Err(TestError("retry-once"))
                } else {
                    Ok("ok")
                }
            }
        })
        .expect("second worker attempt should succeed");

    assert_eq!(value, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(start.elapsed() >= Duration::from_millis(2));
}
