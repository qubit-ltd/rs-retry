// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    mpsc,
};
use std::thread;
use std::time::Duration;

use qubit_clock::{
    ManualBlockingSleeper,
    ManualMonotonicClock,
    MonotonicClock,
};
use qubit_retry::{
    AttemptCancelToken,
    AttemptFailure,
    AttemptFailureDecision,
    AttemptTimeoutOption,
    AttemptTimeoutPolicy,
    AttemptTimeoutSource,
    Retry,
    RetryContext,
    RetryErrorReason,
};

use crate::support::TestError;

/// Counts calls to the reusable worker-thread probe.
static WORKER_THREAD_ID_CALLS: AtomicUsize = AtomicUsize::new(0);
/// Serializes tests that use the reusable worker-thread probe.
static WORKER_THREAD_ID_LOCK: Mutex<()> = Mutex::new(());

/// Returns the current worker thread id and records that the worker ran.
///
/// # Arguments
/// - `token`: Cancellation token for the worker attempt.
///
/// # Returns
/// The current worker thread id.
fn record_worker_thread_id(
    token: AttemptCancelToken,
) -> Result<thread::ThreadId, TestError> {
    assert!(!token.is_cancelled());
    WORKER_THREAD_ID_CALLS.fetch_add(1, Ordering::SeqCst);
    Ok(thread::current().id())
}

/// Verifies worker execution uses a separate thread without timeout settings.
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
        .attempt_timeout_option(Some(AttemptTimeoutOption::retry(
            Duration::from_millis(50),
        )))
        .build()
        .expect("retry should build");

    let worker_thread = retry
        .run_in_worker(record_worker_thread_id)
        .expect("worker attempt should finish before timeout");

    assert_ne!(worker_thread, main_thread);
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 1);
}

/// Verifies an injected blocking sleeper overflow is preserved in worker mode.
#[test]
fn test_run_in_worker_reports_injected_sleeper_failure() {
    let clock = Arc::new(ManualMonotonicClock::new());
    clock
        .advance(Duration::MAX)
        .expect("manual clock should reach its maximum instant");
    let sleeper = Arc::new(ManualBlockingSleeper::from_clock(clock));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_nanos(1))
        .blocking_sleeper(sleeper)
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            Err::<(), _>(TestError("temporary"))
        })
        .expect_err("deadline overflow should terminate worker retry");

    assert_eq!(error.reason(), RetryErrorReason::SleeperFailed);
    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Executor(_))
    ));
}

/// Verifies max elapsed caps an in-flight worker attempt without a configured
/// timeout.
#[test]
fn test_run_in_worker_max_operation_elapsed_caps_in_flight_attempt_without_configured_timeout()
 {
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let worker_release = Arc::clone(&release_rx);
    let (finished_tx, finished_rx) = mpsc::channel();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .worker_cancel_grace(Duration::ZERO)
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(move |_token: AttemptCancelToken| {
            worker_release
                .lock()
                .expect("release receiver should be lockable")
                .recv()
                .expect("test should release the worker");
            finished_tx
                .send(())
                .expect("worker completion should be observable");
            Ok::<_, TestError>("late")
        })
        .expect_err("max elapsed should stop the in-flight worker attempt");
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");

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
}

/// Verifies a worker max-operation timeout notifies failure listeners without
/// permitting another attempt.
#[test]
fn test_run_in_worker_elapsed_timeout_notifies_failure_without_retrying() {
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let sources = Arc::new(Mutex::new(Vec::new()));
    let listener_failures = Arc::clone(&failures);
    let listener_sources = Arc::clone(&sources);
    let retry_events = Arc::clone(&retries);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .worker_cancel_grace(Duration::from_secs(1))
        .no_delay()
        .on_failure(
            move |failure: &AttemptFailure<TestError>,
                  context: &RetryContext| {
                assert!(matches!(failure, AttemptFailure::Timeout));
                listener_failures.fetch_add(1, Ordering::SeqCst);
                listener_sources
                    .lock()
                    .expect("timeout sources should be lockable")
                    .push(context.attempt_timeout_source());
                AttemptFailureDecision::Retry
            },
        )
        .on_retry(
            move |_failure: &AttemptFailure<TestError>,
                  _context: &RetryContext| {
                retry_events.fetch_add(1, Ordering::SeqCst);
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|token: AttemptCancelToken| {
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Ok::<(), TestError>(())
        })
        .expect_err(
            "max-operation elapsed should terminate the worker attempt",
        );

    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.attempts(), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    assert_eq!(
        *sources.lock().expect("timeout sources should be lockable"),
        vec![Some(AttemptTimeoutSource::MaxOperationElapsed)]
    );
}

/// Verifies max total elapsed caps an in-flight worker attempt without a
/// configured timeout.
#[test]
fn test_run_in_worker_max_total_elapsed_caps_in_flight_attempt_without_configured_timeout()
 {
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let worker_release = Arc::clone(&release_rx);
    let (finished_tx, finished_rx) = mpsc::channel();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_total_elapsed(Some(Duration::from_millis(20)))
        .no_delay()
        .worker_cancel_grace(Duration::ZERO)
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(move |_token: AttemptCancelToken| {
            worker_release
                .lock()
                .expect("release receiver should be lockable")
                .recv()
                .expect("test should release the worker");
            finished_tx
                .send(())
                .expect("worker completion should be observable");
            Ok::<_, TestError>("late")
        })
        .expect_err(
            "max total elapsed should stop the in-flight worker attempt",
        );
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");

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
}

/// Verifies a configured timeout policy wins when it equals remaining max
/// elapsed.
#[test]
fn test_run_in_worker_configured_timeout_policy_wins_when_equal_to_remaining_elapsed()
 {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .attempt_timeout(Some(Duration::from_millis(20)))
        .abort_on_timeout()
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|token: AttemptCancelToken| {
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Ok::<_, TestError>("cancelled")
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

/// Verifies ordinary worker failures can retry while max elapsed bounds
/// attempts.
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

/// Verifies owned string worker panic payloads preserve their message.
#[test]
fn test_run_in_worker_owned_string_panic_preserves_message() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|_token: AttemptCancelToken| -> Result<(), TestError> {
            std::panic::panic_any(String::from("owned panic"));
        })
        .expect_err("owned string worker panic should abort");

    let panic = error
        .last_failure()
        .and_then(AttemptFailure::as_panic)
        .expect("terminal failure should be a captured panic");
    assert_eq!(panic.message(), "owned panic");
}

/// Verifies failure listeners can retry captured worker panics.
#[test]
fn test_run_in_worker_panic_can_be_retried_by_listener() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .on_failure(
            |failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                match failure {
                    AttemptFailure::Panic(panic)
                        if panic.message() == "transient panic" =>
                    {
                        AttemptFailureDecision::Retry
                    }
                    _ => AttemptFailureDecision::UseDefault,
                }
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
#[test]
fn test_run_in_worker_can_abort_and_cancel_token() {
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(5),
        )))
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let saw_cancel = Arc::clone(&saw_cancel);
            move |token: AttemptCancelToken| {
                while !token.is_cancelled() {
                    thread::yield_now();
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
    assert!(saw_cancel.load(Ordering::SeqCst));
}

/// Verifies blocking timeout can retry and later return a successful result.
#[test]
fn test_run_in_worker_retries_timeout_until_success() {
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
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            move |token: AttemptCancelToken| {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    while !token.is_cancelled() {
                        thread::yield_now();
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
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
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
    let error = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            let release_rx = Arc::clone(&release_rx);
            move |_token: AttemptCancelToken| {
                attempts.fetch_add(1, Ordering::SeqCst);
                release_rx
                    .lock()
                    .expect("release receiver should be lockable")
                    .recv()
                    .expect("test should release the worker");
                finished_tx
                    .send(())
                    .expect("worker completion should be observable");
                Ok::<_, TestError>("late")
            }
        })
        .expect_err("unreaped timeout worker should stop retries");
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(error.unreaped_worker_count(), 1);
    assert_eq!(error.context().unreaped_worker_count(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies an unreaped worker timeout reports the worker-specific terminal
/// reason.
#[test]
fn test_run_in_worker_unreaped_timeout_worker_reason_wins_over_attempts_exceeded()
 {
    let attempts = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::new(
            Duration::from_millis(5),
            AttemptTimeoutPolicy::Retry,
        )))
        .worker_cancel_grace(Duration::from_millis(5))
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let attempts = Arc::clone(&attempts);
            let release_rx = Arc::clone(&release_rx);
            move |_token: AttemptCancelToken| {
                attempts.fetch_add(1, Ordering::SeqCst);
                release_rx
                    .lock()
                    .expect("release receiver should be lockable")
                    .recv()
                    .expect("test should release the worker");
                finished_tx
                    .send(())
                    .expect("worker completion should be observable");
                Ok::<_, TestError>("late")
            }
        })
        .expect_err(
            "unreaped timeout worker should report worker still running",
        );
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(error.unreaped_worker_count(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies a timed-out worker that exits during cancellation grace is reaped.
#[test]
fn test_run_in_worker_timeout_reaps_cooperative_worker_during_grace() {
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(5),
        )))
        .worker_cancel_grace(Duration::from_millis(100))
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker({
            let saw_cancel = Arc::clone(&saw_cancel);
            move |token: AttemptCancelToken| {
                while !token.is_cancelled() {
                    thread::yield_now();
                }
                saw_cancel.store(true, Ordering::SeqCst);
                Err::<(), TestError>(TestError("cancelled"))
            }
        })
        .expect_err("timeout should abort even when worker exits during grace");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(error.unreaped_worker_count(), 0);
    assert_eq!(error.context().unreaped_worker_count(), 0);
    assert!(saw_cancel.load(Ordering::SeqCst));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
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

/// Verifies worker mode honors max total elapsed before running the first
/// attempt.
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

    let error = retry.run_in_worker(record_worker_thread_id).expect_err(
        "zero total elapsed budget should stop before first attempt",
    );

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.context().attempt_timeout(), Some(Duration::ZERO));
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxTotalElapsed)
    );
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
}

/// Verifies worker mode includes before-attempt listener time in max total
/// elapsed.
#[test]
fn test_run_in_worker_max_total_elapsed_includes_before_attempt_listener_time()
{
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper =
        Arc::new(ManualBlockingSleeper::from_clock(Arc::clone(&clock)));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_sleeper(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            listener_clock
                .advance(Duration::from_secs(20))
                .expect("manual time should advance");
        })
        .build()
        .expect("retry should build");

    let error = retry.run_in_worker(record_worker_thread_id).expect_err(
        "before-attempt listener time should exhaust total elapsed",
    );

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1]
    );
    assert!(error.last_failure().is_none());
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}

/// Verifies worker mode sleeps when retrying with non-zero delay.
#[test]
fn test_run_in_worker_retries_with_non_zero_delay() {
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper =
        Arc::new(ManualBlockingSleeper::from_clock(Arc::clone(&clock)));
    let attempts = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_secs(2))
        .blocking_sleeper(sleeper)
        .build()
        .expect("retry should build");

    let worker = std::thread::spawn({
        let attempts = Arc::clone(&attempts);
        move || {
            retry
                .run_in_worker({
                    let attempts = Arc::clone(&attempts);
                    move |_token: AttemptCancelToken| -> Result<&'static str, TestError> {
                        let attempt =
                            attempts.fetch_add(1, Ordering::SeqCst) + 1;
                        if attempt == 1 {
                            Err(TestError("retry-once"))
                        } else {
                            Ok("ok")
                        }
                    }
                })
                .expect("second worker attempt should succeed")
        }
    });

    assert!(clock.wait_for_waiters(1, Duration::from_secs(1)));
    assert_eq!(
        clock
            .next_deadline()
            .expect("worker retry delay should register a deadline")
            .duration_since(clock.now())
            .expect("deadline should share the manual clock domain"),
        Duration::from_secs(2)
    );
    clock
        .advance(Duration::from_secs(2))
        .expect("manual time should advance");
    let value = worker.join().expect("retry thread should join");

    assert_eq!(value, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(2));
}

/// Verifies worker runner result and timeout paths through public APIs.
#[test]
fn test_worker_retry_runner_paths_are_observable_through_timeout_and_success() {
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(20)))
        .worker_cancel_grace(Duration::from_millis(20))
        .build()
        .expect("retry should build");

    assert_eq!(
        "ok",
        retry
            .run_in_worker(|_token: AttemptCancelToken| Ok("ok"))
            .expect("worker attempt should succeed")
    );

    let error = retry
        .run_in_worker(move |_token: AttemptCancelToken| {
            release_rx
                .lock()
                .expect("release receiver should be lockable")
                .recv()
                .expect("test should release the worker");
            finished_tx
                .send(())
                .expect("worker completion should be observable");
            Err::<&'static str, &'static str>("late")
        })
        .unwrap_err();
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");
    assert_eq!(error.reason(), RetryErrorReason::WorkerStillRunning);
}
