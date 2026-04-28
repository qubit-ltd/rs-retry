/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken, AttemptFailure, AttemptFailureDecision, AttemptTimeoutOption,
    AttemptTimeoutPolicy, AttemptTimeoutSource, Retry, RetryContext, RetryErrorReason,
};

use crate::support::TestError;

/// Counts calls to the reusable worker-thread probe.
static WORKER_THREAD_ID_CALLS: AtomicUsize = AtomicUsize::new(0);
/// Serializes tests that use the reusable worker-thread probe.
static WORKER_THREAD_ID_LOCK: Mutex<()> = Mutex::new(());

/// Returns the current worker thread id and records that the worker ran.
///
/// # Parameters
/// - `token`: Cancellation token for the worker attempt.
///
/// # Returns
/// The current worker thread id.
fn record_worker_thread_id(token: AttemptCancelToken) -> Result<thread::ThreadId, TestError> {
    assert!(!token.is_cancelled());
    WORKER_THREAD_ID_CALLS.fetch_add(1, Ordering::SeqCst);
    Ok(thread::current().id())
}

/// Verifies worker execution uses a separate thread without timeout settings.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_executes_on_worker_without_timeout() {
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let main_thread = thread::current().id();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let worker_thread = retry
        .run_in_worker(record_worker_thread_id)
        .expect("worker attempt should succeed");

    assert_ne!(worker_thread, main_thread);
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 1);
}

/// Verifies worker execution with a timeout can complete before the deadline.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_with_timeout_allows_fast_success() {
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let main_thread = thread::current().id();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::retry(Duration::from_millis(50))))
        .build()
        .expect("retry should build");

    let worker_thread = retry
        .run_in_worker(record_worker_thread_id)
        .expect("worker attempt should finish before timeout");

    assert_ne!(worker_thread, main_thread);
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 1);
}

/// Verifies max elapsed caps an in-flight worker attempt without a configured timeout.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_max_operation_elapsed_caps_in_flight_attempt_without_configured_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .worker_cancel_grace(Duration::ZERO)
        .build()
        .expect("retry should build");

    let started = std::time::Instant::now();
    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            thread::sleep(Duration::from_millis(120));
            Ok::<_, TestError>("late")
        })
        .expect_err("max elapsed should stop the in-flight worker attempt");
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
        "max elapsed should stop before the worker finishes, elapsed: {elapsed:?}"
    );
}

/// Verifies max total elapsed caps an in-flight worker attempt without a configured timeout.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_max_total_elapsed_caps_in_flight_attempt_without_configured_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .worker_cancel_grace(Duration::ZERO)
        .build()
        .expect("retry should build");

    let started = std::time::Instant::now();
    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            thread::sleep(Duration::from_millis(120));
            Ok::<_, TestError>("late")
        })
        .expect_err("max total elapsed should stop the in-flight worker attempt");
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
        "max total elapsed should stop before the worker finishes, elapsed: {elapsed:?}"
    );
}

/// Verifies a configured timeout policy wins when it equals remaining max elapsed.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_configured_timeout_policy_wins_when_equal_to_remaining_elapsed() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout(Some(Duration::from_millis(20)))
        .abort_on_timeout()
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            thread::sleep(Duration::from_millis(120));
            Ok::<_, TestError>("late")
        })
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

/// Verifies ordinary worker failures can retry while max elapsed bounds attempts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_run_in_worker_error_before_remaining_elapsed_timeout_can_retry() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(200)))
        .no_delay()
        .build()
        .expect("retry should build");
    let attempts = Arc::new(AtomicUsize::new(0));
    let operation_attempts = Arc::clone(&attempts);

    let value = retry
        .run_in_worker(move |_token: AttemptCancelToken| {
            if operation_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("transient"))
            } else {
                Ok("done")
            }
        })
        .expect("ordinary error should retry before remaining elapsed timeout");

    assert_eq!(value, "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
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
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
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
            move |token: AttemptCancelToken| {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    while !token.is_cancelled() {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err::<&'static str, TestError>(TestError("cancelled"))
                } else {
                    Ok::<_, TestError>("done")
                }
            }
        })
        .expect("second blocking attempt should succeed");

    assert_eq!(value, "done");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies a timed-out worker that ignores cancellation stops retries.
#[test]
fn test_run_in_worker_unreaped_timeout_worker_stops_retrying() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::new(
            Duration::from_millis(5),
            AttemptTimeoutPolicy::Retry,
        )))
        .worker_cancel_grace(Duration::from_millis(5))
        .build()
        .expect("retry should build");
    let start = std::time::Instant::now();

    let error = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |_token: AttemptCancelToken| {
                attempts.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(120));
                Ok::<_, TestError>("late")
            }
        })
        .expect_err("unreaped timeout worker should stop retries");

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(error.unreaped_worker_count(), 1);
    assert_eq!(error.context().unreaped_worker_count(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "retry should not wait for the uncooperative worker to finish"
    );
}

/// Verifies worker mode honors max elapsed before running the first attempt.
#[test]
fn test_run_in_worker_max_operation_elapsed_can_stop_before_first_attempt() {
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(record_worker_thread_id)
        .expect_err("zero elapsed budget should stop before first attempt");

    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.context().attempt_timeout(), Some(Duration::ZERO));
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxOperationElapsed)
    );
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
}

/// Verifies worker mode honors max total elapsed before running the first attempt.
#[test]
fn test_run_in_worker_max_total_elapsed_can_stop_before_first_attempt() {
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(record_worker_thread_id)
        .expect_err("zero total elapsed budget should stop before first attempt");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.context().attempt_timeout(), Some(Duration::ZERO));
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxTotalElapsed)
    );
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
}

/// Verifies worker mode includes before-attempt listener time in max total elapsed.
#[test]
fn test_run_in_worker_max_total_elapsed_includes_before_attempt_listener_time() {
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .before_attempt(|_context: &RetryContext| {
            thread::sleep(Duration::from_millis(40));
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(record_worker_thread_id)
        .expect_err("before-attempt listener time should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert!(error.last_failure().is_none());
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
    assert!(error.context().total_elapsed() >= Duration::from_millis(20));
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
