// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "tokio")]

use std::sync::{
    Arc,
    Mutex,
    atomic::{
        AtomicUsize,
        Ordering,
    },
};
use std::time::Duration;

use qubit_clock::{
    ManualMonotonicClock,
    MonotonicClock,
    TokioTimer,
};
use qubit_retry::{
    AttemptFailure,
    AttemptFailureDecision,
    AttemptTimeoutSource,
    Retry,
    RetryContext,
    RetryError,
    RetryErrorReason,
};

use crate::support::TestError;

/// Verifies that the default async timer binds to a paused runtime when the
/// retry future is first polled, rather than when the policy is built.
///
/// # Panics
///
/// Panics if the retry policy cannot be built or the async retry fails.
#[test]
fn test_run_async_default_timer_binds_on_first_poll() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("paused Tokio runtime should build");
    // This real pre-runtime delay distinguishes construction time from the
    // first-poll clock binding that the test exercises.
    std::thread::sleep(Duration::from_millis(10));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_secs(5))
        .build()
        .expect("retry should build outside the runtime");

    let (result, runtime_elapsed) = runtime.block_on(async {
        let started_at = tokio::time::Instant::now();
        let mut attempts = 0;
        let result = retry
            .run_async(|| {
                attempts += 1;
                let attempt = attempts;
                async move {
                    if attempt == 1 {
                        Err(TestError("temporary"))
                    } else {
                        Ok(attempt)
                    }
                }
            })
            .await;
        (result, tokio::time::Instant::now() - started_at)
    });

    assert_eq!(result.expect("second attempt should succeed"), 2);
    assert_eq!(runtime_elapsed, Duration::from_secs(5));
}

/// Verifies that an injected Tokio timer retains its target runtime while the
/// retry future is polled by a different runtime.
#[test]
fn test_run_async_uses_injected_tokio_timer_across_runtimes() {
    let target = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("target runtime should build");
    let polling = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("polling runtime should build");
    let timer = target.block_on(async { Arc::new(TokioTimer::current()) });
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_secs(5))
        .async_timer(timer)
        .build()
        .expect("retry should build");
    let attempts = Arc::new(AtomicUsize::new(0));
    let operation_attempts = Arc::clone(&attempts);
    let mut retry_future = Box::pin(retry.run_async(move || {
        let attempt = operation_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if attempt == 1 {
                Err(TestError("temporary"))
            } else {
                Ok(attempt)
            }
        }
    }));

    let early_result = polling.block_on(async {
        tokio::select! {
            result = &mut retry_future => Some(result),
            () = tokio::time::sleep(Duration::from_secs(1)) => None,
        }
    });
    assert!(
        early_result.is_none(),
        "advancing the polling runtime must not complete the backoff"
    );

    target.block_on(tokio::time::advance(Duration::from_secs(5)));
    assert_eq!(
        polling
            .block_on(retry_future)
            .expect("second attempt should succeed"),
        2,
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

/// Verifies async attempt timeout is driven by injected manual time.
#[tokio::test]
async fn test_run_async_attempt_timeout_uses_injected_async_timer() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_secs(30)))
        .abort_on_timeout()
        .async_timer(sleeper.clone())
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(clock.pending_waiters(), 1);

    assert_eq!(Duration::from_secs(30), reached.elapsed_since_origin(),);
    let error = retry_future
        .await
        .expect_err("manual timeout should abort the attempt");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(error.context().attempt_elapsed(), Duration::from_secs(30));
}

/// Verifies async retry backoff is driven by injected manual time.
#[tokio::test]
async fn test_run_async_backoff_uses_injected_async_timer() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::from_secs(5))
        .async_timer(sleeper.clone())
        .build()
        .expect("retry should build");
    let attempts = Arc::new(AtomicUsize::new(0));
    let operation_attempts = Arc::clone(&attempts);
    let retry_future = retry.run_async(move || {
        let attempt = operation_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if attempt < 3 {
                Err(TestError("temporary"))
            } else {
                Ok(attempt)
            }
        }
    });
    tokio::pin!(retry_future);
    for stage in 1_u64..=2 {
        let reached = tokio::select! {
            result = &mut retry_future => {
                panic!(
                    "retry completed before backoff stage {stage}: {result:?}"
                );
            }
            reached = clock.advance_to_next_deadline_async() => reached,
        };
        assert_eq!(clock.pending_waiters(), 1);

        assert_eq!(
            Duration::from_secs(stage * 5),
            reached.elapsed_since_origin(),
        );
    }

    assert_eq!(retry_future.await.expect("third attempt should succeed"), 3);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

/// Verifies an injected async timer overflow becomes a typed error.
#[tokio::test]
async fn test_run_async_reports_injected_timer_failure() {
    let clock = ManualMonotonicClock::new_shared();
    clock
        .advance(Duration::MAX)
        .expect("manual clock should reach its maximum instant");
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .fixed_delay(Duration::from_nanos(1))
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async { Err::<(), _>(TestError("temporary")) })
        .await
        .expect_err("deadline overflow should terminate retry");

    assert_eq!(error.reason(), RetryErrorReason::SleeperFailed);
    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Executor(_))
    ));
}

/// Verifies async operation panic still propagates through the current task.
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
#[tokio::test]
async fn test_run_async_attempt_timeout_can_abort() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .attempt_timeout(Some(Duration::from_secs(1)))
        .abort_on_timeout()
        .no_delay()
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(1), reached.elapsed_since_origin(),);
    let error = retry_future.await.expect_err("timeout should abort");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::Configured)
    );
}

/// Verifies max elapsed caps an in-flight async attempt before a configured
/// timeout.
#[tokio::test]
async fn test_run_async_max_operation_elapsed_caps_in_flight_attempt_before_configured_timeout()
 {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_secs(20)))
        .attempt_timeout(Some(Duration::from_secs(200)))
        .no_delay()
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<&str, TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(20), reached.elapsed_since_origin(),);
    let error = retry_future
        .await
        .expect_err("max elapsed should stop the in-flight async attempt");

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
        Some(Duration::from_secs(20))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxOperationElapsed)
    );
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(20));
}

/// Verifies max total elapsed caps an in-flight async attempt before a
/// configured timeout.
#[tokio::test]
async fn test_run_async_max_total_elapsed_caps_in_flight_attempt_before_configured_timeout()
 {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .attempt_timeout(Some(Duration::from_secs(200)))
        .no_delay()
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<&str, TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(20), reached.elapsed_since_origin(),);
    let error = retry_future.await.expect_err(
        "max total elapsed should stop the in-flight async attempt",
    );

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_secs(20))
    );
    assert_eq!(
        error.context().attempt_timeout_source(),
        Some(AttemptTimeoutSource::MaxTotalElapsed)
    );
    assert_eq!(clock.now().elapsed_since_origin(), Duration::from_secs(20));
}

/// Verifies a max-operation timeout is fully observed without allowing a
/// listener decision to retry it.
#[tokio::test]
async fn test_run_async_elapsed_timeout_notifies_failure_without_retrying() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let hints = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let terminal_errors = Arc::new(AtomicUsize::new(0));
    let sources = Arc::new(Mutex::new(Vec::new()));
    let hint_events = Arc::clone(&hints);
    let listener_failures = Arc::clone(&failures);
    let listener_sources = Arc::clone(&sources);
    let retry_events = Arc::clone(&retries);
    let error_events = Arc::clone(&terminal_errors);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_operation_elapsed(Some(Duration::from_secs(30)))
        .no_delay()
        .async_timer(sleeper)
        .retry_after_hint(
            move |failure: &AttemptFailure<TestError>,
                  context: &RetryContext| {
                assert!(matches!(failure, AttemptFailure::Timeout));
                assert_eq!(
                    context.attempt_timeout_source(),
                    Some(AttemptTimeoutSource::MaxOperationElapsed)
                );
                hint_events.fetch_add(1, Ordering::SeqCst);
                Some(Duration::from_secs(99))
            },
        )
        .on_failure(
            move |failure: &AttemptFailure<TestError>,
                  context: &RetryContext| {
                assert!(matches!(failure, AttemptFailure::Timeout));
                assert_eq!(
                    context.retry_after_hint(),
                    Some(Duration::from_secs(99))
                );
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
        .on_error(
            move |_error: &RetryError<TestError>, _context: &RetryContext| {
                error_events.fetch_add(1, Ordering::SeqCst);
            },
        )
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(30), reached.elapsed_since_origin(),);

    let error = retry_future
        .await
        .expect_err("max-operation elapsed should terminate the attempt");
    assert_eq!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded
    );
    assert_eq!(error.attempts(), 1);
    assert_eq!(hints.load(Ordering::SeqCst), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    assert_eq!(terminal_errors.load(Ordering::SeqCst), 1);
    assert_eq!(
        *sources.lock().expect("timeout sources should be lockable"),
        vec![Some(AttemptTimeoutSource::MaxOperationElapsed)]
    );
    assert_eq!(
        error.context().retry_after_hint(),
        Some(Duration::from_secs(99))
    );
}

/// Verifies a max-total timeout is fully observed and remains terminal even
/// when a failure listener asks to abort.
#[tokio::test]
async fn test_run_async_total_timeout_notifies_failure_without_retrying() {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let sources = Arc::new(Mutex::new(Vec::new()));
    let listener_failures = Arc::clone(&failures);
    let listener_sources = Arc::clone(&sources);
    let retry_events = Arc::clone(&retries);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_total_elapsed(Some(Duration::from_secs(30)))
        .no_delay()
        .async_timer(sleeper)
        .on_failure(
            move |failure: &AttemptFailure<TestError>,
                  context: &RetryContext| {
                assert!(matches!(failure, AttemptFailure::Timeout));
                listener_failures.fetch_add(1, Ordering::SeqCst);
                listener_sources
                    .lock()
                    .expect("timeout sources should be lockable")
                    .push(context.attempt_timeout_source());
                AttemptFailureDecision::Abort
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

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(30), reached.elapsed_since_origin(),);

    let error = retry_future
        .await
        .expect_err("max-total elapsed should terminate the attempt");
    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    assert_eq!(
        *sources.lock().expect("timeout sources should be lockable"),
        vec![Some(AttemptTimeoutSource::MaxTotalElapsed)]
    );
}

/// Verifies a shorter configured timeout still wins over remaining max elapsed.
#[tokio::test]
async fn test_run_async_configured_timeout_wins_when_shorter_than_max_operation_elapsed()
 {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .max_operation_elapsed(Some(Duration::from_secs(200)))
        .attempt_timeout(Some(Duration::from_secs(20)))
        .abort_on_timeout()
        .no_delay()
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<&str, TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(20), reached.elapsed_since_origin(),);
    let error = retry_future
        .await
        .expect_err("configured attempt timeout should abort first");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_secs(20))
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

/// Verifies a configured timeout policy wins when it equals remaining max
/// elapsed.
#[tokio::test]
async fn test_run_async_configured_timeout_policy_wins_when_equal_to_remaining_elapsed()
 {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_secs(20)))
        .attempt_timeout(Some(Duration::from_secs(20)))
        .abort_on_timeout()
        .no_delay()
        .async_timer(sleeper)
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<&str, TestError>>);
    tokio::pin!(retry_future);
    let reached = tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(Duration::from_secs(20), reached.elapsed_since_origin(),);
    let error = retry_future
        .await
        .expect_err("configured timeout policy should abort on equal timeout");

    assert_eq!(error.reason(), RetryErrorReason::Aborted);
    assert_eq!(
        error.context().attempt_timeout(),
        Some(Duration::from_secs(20))
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

/// Verifies ordinary async failures can retry while max elapsed bounds
/// attempts.
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

/// Verifies async retry succeeds without per-attempt timeout after a retry
/// delay.
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

/// Verifies async execution can stop before the first attempt on elapsed
/// budget.
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

/// Verifies async execution includes before-attempt listener time in max total
/// elapsed.
#[tokio::test]
async fn test_run_async_max_total_elapsed_includes_before_attempt_listener_time()
 {
    let clock = ManualMonotonicClock::new_shared();
    let sleeper = clock.new_timer();
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .async_timer(sleeper)
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
        .run_async::<(), _, _>(|| async { panic!("operation must not run") })
        .await
        .expect_err(
            "before-attempt listener time should exhaust total elapsed",
        );

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

/// Verifies async retry handles zero retry delay without sleeping.
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
