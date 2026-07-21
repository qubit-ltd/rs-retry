// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::panic;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qubit_clock::{ManualMonotonicClock, MonotonicClock};
use qubit_error::BoxError;
use qubit_retry::{
    AttemptFailure, AttemptTimeoutSource, Retry, RetryContext, RetryError, RetryErrorReason,
};

use crate::support::{NonCloneValue, TestError};

/// Verifies exponential backoff is driven entirely by injected manual time.
#[test]
fn test_run_exponential_backoff_uses_injected_blocking_timer() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .exponential_backoff(Duration::from_secs(1), Duration::from_secs(8))
        .blocking_timer(sleeper.clone())
        .build()
        .expect("retry should build");

    let worker = std::thread::spawn(move || {
        let mut attempts = 0;
        let result = retry.run(|| {
            attempts += 1;
            if attempts < 3 {
                Err(TestError("temporary"))
            } else {
                Ok(attempts)
            }
        });
        (result, attempts)
    });

    assert!(clock.wait_for_waiters(1, Duration::from_secs(1)));
    assert_eq!(
        clock
            .next_deadline()
            .expect("first backoff deadline should be registered")
            .duration_since(clock.now())
            .expect("deadline should share the manual domain"),
        Duration::from_secs(1)
    );
    clock
        .advance(Duration::from_secs(1))
        .expect("manual time should advance");

    let second_deadline = clock
        .wait_for_next_deadline(Duration::from_secs(1))
        .expect("second backoff deadline should be registered");
    assert!(
        !worker.is_finished(),
        "retry thread finished before the second backoff advanced",
    );
    assert_eq!(
        second_deadline
            .duration_since(clock.now())
            .expect("deadline should share the manual domain"),
        Duration::from_secs(2)
    );
    clock
        .advance(Duration::from_secs(2))
        .expect("manual time should advance");

    let (result, attempts) = worker.join().expect("retry thread should join");
    assert_eq!(result.expect("third attempt should succeed"), 3);
    assert_eq!(attempts, 3);
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(3));
}

/// Verifies an injected blocking timer overflow becomes a typed error.
#[test]
fn test_run_reports_injected_blocking_timer_failure() {
    let clock = ManualMonotonicClock::new_shared();
    clock
        .advance(Duration::MAX)
        .expect("manual clock should reach its maximum instant");
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_nanos(1))
        .blocking_timer(sleeper)
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("temporary")) })
        .expect_err("deadline overflow should terminate retry");

    assert_eq!(error.reason(), RetryErrorReason::SleeperFailed);
    assert_eq!(error.attempts(), 1);
    assert!(
        error
            .last_failure()
            .and_then(AttemptFailure::as_executor_error)
            .expect("sleeper failure should be preserved")
            .message()
            .contains("instant overflow")
    );
    assert_eq!(
        error.to_string(),
        "retry sleeper failed after 1 attempt(s); last failure: attempt executor failed: retry sleeper failed: monotonic instant overflow"
    );
}

/// Verifies sync retry succeeds and emits attempt lifecycle events.
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

/// Verifies sync execution rejects configured attempt timeout.
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
#[test]
fn test_hook_and_retry_sleep_time_do_not_count_against_elapsed_budget() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let success_elapsed = Arc::new(Mutex::new(None));
    let success_elapsed_events = Arc::clone(&success_elapsed);
    let before_clock = Arc::clone(&clock);
    let retry_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_secs(10)))
        .fixed_delay(Duration::from_secs(25))
        .blocking_timer(sleeper)
        .before_attempt(move |_context: &RetryContext| {
            before_clock
                .advance(Duration::from_secs(25))
                .expect("manual time should advance");
        })
        .on_retry(
            move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                retry_clock
                    .advance(Duration::from_secs(25))
                    .expect("manual time should advance");
            },
        )
        .on_success(move |context: &RetryContext| {
            *success_elapsed_events
                .lock()
                .expect("success elapsed should be lockable") = Some(context.operation_elapsed());
        })
        .build()
        .expect("retry should build");

    let worker = std::thread::spawn(move || {
        let mut attempts = 0;
        let value = retry.run(|| {
            attempts += 1;
            if attempts == 1 {
                Err(TestError("retry-once"))
            } else {
                Ok("done")
            }
        });
        (value, attempts)
    });

    assert!(clock.wait_for_waiters(1, Duration::from_secs(1)));
    assert_eq!(
        clock
            .next_deadline()
            .expect("retry delay should register a deadline")
            .duration_since(clock.now())
            .expect("deadline should share the manual clock domain"),
        Duration::from_secs(25)
    );
    clock
        .advance(Duration::from_secs(25))
        .expect("manual time should advance");
    let (value, attempts) = worker.join().expect("retry thread should join");
    let value = value.expect("hook and retry sleep time should not exhaust elapsed budget");

    assert_eq!(value, "done");
    assert_eq!(attempts, 2);
    assert_eq!(
        success_elapsed
            .lock()
            .expect("success elapsed should be lockable")
            .expect("success listener should run"),
        Duration::ZERO
    );
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(100));
}

/// Verifies total elapsed budget rejects a retry delay before sleeping it.
#[test]
fn test_max_total_elapsed_rejects_retry_sleep_before_sleeping() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let scheduled_events = Arc::clone(&retry_events);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(50)))
        .fixed_delay(Duration::from_secs(200))
        .blocking_timer(sleeper)
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
    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("retry-delay-too-large"))
        })
        .expect_err("total elapsed budget should reject the retry delay");
    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(attempts, 1);
    assert_eq!(
        error.last_error(),
        Some(&TestError("retry-delay-too-large"))
    );
    assert_eq!(error.context().next_delay(), Some(Duration::from_secs(200)));
    assert!(
        retry_events
            .lock()
            .expect("retry events should be lockable")
            .is_empty()
    );
    assert_eq!(clock.pending_waiters(), 0);
    assert_eq!(clock.now().elapsed_since_origin(), Duration::ZERO);
}

/// Verifies retry-after delay participates in the total elapsed budget.
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

/// Verifies before-attempt listener time can exhaust total elapsed before
/// operation runs.
#[test]
fn test_max_total_elapsed_includes_before_attempt_listener_time() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_timer(sleeper)
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

    let error = retry
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("before-attempt listener time should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1]
    );
    assert!(error.last_failure().is_none());
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}

/// Verifies a pre-attempt budget stop preserves the preceding committed
/// attempt and failure.
#[test]
fn test_max_total_elapsed_before_second_operation_preserves_first_attempt() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_timer(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            if context.attempt() == 2 {
                listener_clock
                    .advance(Duration::from_secs(20))
                    .expect("manual time should advance");
            }
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("first")) })
        .expect_err("second pre-attempt check should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("first")));
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1, 2]
    );
}
