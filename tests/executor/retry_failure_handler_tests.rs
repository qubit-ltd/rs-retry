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
    AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryError, RetryErrorReason,
};

use crate::support::TestError;

/// Verifies the default boxed error type exercises listener and retry-delay
/// paths.
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

/// Verifies retry listener time does not count against elapsed budget.
#[test]
fn test_on_retry_listener_time_does_not_count_against_elapsed_budget() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let scheduled_events = Arc::clone(&retry_events);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_secs(10)))
        .fixed_delay(Duration::from_secs(25))
        .blocking_timer(sleeper)
        .on_retry(
            move |failure: &AttemptFailure<TestError>, context: &RetryContext| {
                scheduled_events
                    .lock()
                    .expect("retry events should be lockable")
                    .push((failure.as_error().cloned(), context.next_delay()));
                listener_clock
                    .advance(Duration::from_secs(25))
                    .expect("manual time should advance");
            },
        )
        .build()
        .expect("retry should build");

    let worker = std::thread::spawn(move || {
        let mut attempts = 0;
        let error = retry.run(|| -> Result<(), TestError> {
            attempts += 1;
            Err(TestError("slow-listener"))
        });
        (error, attempts)
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
    let (error, attempts) = worker.join().expect("retry thread should join");
    let error =
        error.expect_err("attempts should be exhausted after listener and sleep time are excluded");

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
            Some(Duration::from_secs(25))
        )]
    );
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(50));
}

/// Verifies retry control-path listener time is included in total elapsed.
#[test]
fn test_max_total_elapsed_includes_failure_listener_time() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_timer(sleeper)
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                listener_clock
                    .advance(Duration::from_secs(20))
                    .expect("manual time should advance");
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
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}

/// Verifies on-retry listener time can exhaust total elapsed before retrying.
#[test]
fn test_max_total_elapsed_includes_on_retry_listener_time() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_timer(sleeper)
        .on_retry(
            move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                listener_clock
                    .advance(Duration::from_secs(20))
                    .expect("manual time should advance");
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
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}

/// Verifies on-retry listener time can leave too little total elapsed for the
/// selected sleep.
#[test]
fn test_max_total_elapsed_rechecks_retry_sleep_after_on_retry_listener() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(80)))
        .fixed_delay(Duration::from_secs(50))
        .blocking_timer(sleeper)
        .on_retry(
            move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                listener_clock
                    .advance(Duration::from_secs(40))
                    .expect("manual time should advance");
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
    assert_eq!(error.context().next_delay(), Some(Duration::from_secs(50)));
    assert_eq!(clock.pending_waiters(), 0);
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(40));
}
