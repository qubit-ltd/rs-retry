// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[cfg(feature = "tokio")]
use qubit_clock::ManualMonotonicClock;
#[cfg(feature = "tokio")]
use qubit_clock::MonotonicClock;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestError;

impl std::fmt::Display for TestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("test error")
    }
}

impl std::error::Error for TestError {}

/// Runs one failing operation to produce a public terminal retry error.
fn create_exhausted_error() -> RetryError<TestError> {
    Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("terminal-accessor policy should be valid"),
    )
    .build()
    .sync()
    .run::<(), _>(|| Err(TestError))
    .expect_err("one failed attempt should exhaust the policy")
}

/// Verifies public retry-error accessors retain all terminal information.
#[test]
fn test_retry_error_terminal_accessors_are_lossless() {
    let error = create_exhausted_error();
    assert_eq!(error.last_failure(), error.failure().last_failure());
    assert_eq!(error.last_error(), Some(&TestError));
    let failure = error.into_failure();
    assert!(matches!(
        failure,
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            last_failure: Some(AttemptFailure::Error(TestError)),
            ..
        }
    ));

    let error = create_exhausted_error();
    let (failure, context) = error.into_parts();
    assert_eq!(failure.last_error(), Some(&TestError));
    assert_eq!(context.attempts(), 1);
    assert_eq!(context.current_attempt(), None);
}

/// Verifies public component constructors expose their complete stable data.
#[test]
fn test_public_failure_component_constructors_and_accessors() {
    let context = RetryContext::new(2, 4);
    assert_eq!(context.attempts(), 2);
    assert_eq!(
        context.current_attempt().map(std::num::NonZeroU32::get),
        Some(2)
    );
    assert_eq!(context.max_attempts(), 4);

    let callback = RetryCallbackFailure::new(
        RetryCallbackKind::Observer,
        3,
        RetryCallbackPhase::RetryScheduled,
        RetryPanic::String("observer panic".to_owned()),
    );
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 3);
    assert_eq!(callback.phase(), RetryCallbackPhase::RetryScheduled);
    assert_eq!(callback.panic().message(), Some("observer panic"));

    let attempt = AttemptFailure::<TestError>::TimedOut {
        scope: RetryTimeoutScope::Flow,
    };
    assert!(attempt.is_timeout());
    assert_eq!(attempt.timeout_scope(), Some(RetryTimeoutScope::Flow));
    assert_eq!(attempt.as_error(), None);

    let infrastructure = RetryInfrastructureFailure::WorkerStillRunning {
        trigger: WorkerStopTrigger::Cancellation,
    };
    assert_eq!(infrastructure.message(), None);
    assert_eq!(
        infrastructure.worker_stop_trigger(),
        Some(WorkerStopTrigger::Cancellation)
    );

    let cancellation = RetryCancellationToken::new();
    assert!(!cancellation.is_cancelled());
    cancellation.cancel();
    assert!(cancellation.is_cancelled());
}

#[test]
fn policy_validates_limits_and_backoff_state() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(10),
        2.0,
        Duration::from_millis(25),
    )
    .unwrap();
    let mut state = policy.start();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(20)
    );
    state.reset();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
}

#[test]
fn sync_facade_retries_application_failure() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = AtomicU32::new(0);
    let result = retry.sync().run(|| {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(TestError)
        } else {
            Ok(42_u32)
        }
    });
    let success = result.expect("second attempt succeeds");
    assert_eq!(*success.value(), 42);
    assert_eq!(success.context().attempts(), 2);
}

#[test]
fn first_rule_wins_and_failure_kind_is_stable() {
    struct RetryOnly;
    impl RetryRule<TestError> for RetryOnly {
        fn decide(
            &self,
            _: &AttemptFailure<TestError>,
            _: &RetryContext,
        ) -> RetryDecision {
            RetryDecision::Retry
        }
    }
    struct AbortRule;
    impl RetryRule<TestError> for AbortRule {
        fn decide(
            &self,
            _: &AttemptFailure<TestError>,
            _: &RetryContext,
        ) -> RetryDecision {
            RetryDecision::Abort
        }
    }
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy)
        .rule(RetryOnly)
        .rule(AbortRule)
        .build();
    let error = retry.sync().run::<(), _>(|| Err(TestError)).unwrap_err();
    assert!(matches!(
        error.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            last_failure: Some(AttemptFailure::Error(TestError)),
            ..
        }
    ));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_facade_retries_and_preserves_success() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let result = retry
        .asynchronous()
        .run({
            let attempts = Arc::clone(&attempts);
            move || {
                let attempts = Arc::clone(&attempts);
                async move {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err(TestError)
                    } else {
                        Ok(7_u32)
                    }
                }
            }
        })
        .await
        .expect("second attempt succeeds");
    assert_eq!(*result.value(), 7);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let error = retry
        .asynchronous()
        .attempt_timeout(Duration::from_millis(1))
        .run(|| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Err::<(), _>(TestError)
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Attempt
            }),
            ..
        }
    ));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_shorter_flow_timeout_reports_flow_source() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let clock = ManualMonotonicClock::new_shared();
    let executor = retry
        .asynchronous()
        .attempt_timeout(Duration::from_secs(30))
        .flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer());
    let future = executor.run(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(future);

    let reached = tokio::select! {
        result = &mut future => {
            panic!("retry completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(reached.elapsed_since_origin(), Duration::from_secs(1));

    let error = future
        .await
        .expect_err("flow timeout should terminate retry");
    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Flow
            }),
            ..
        }
    ));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_flow_timeout_caps_retry_sleep() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::from_secs(30)))
        .build()
        .unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let clock = ManualMonotonicClock::new_shared();
    let attempts = Arc::new(AtomicU32::new(0));
    let executor = retry
        .asynchronous()
        .flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer());
    let future = executor.run({
        let attempts = Arc::clone(&attempts);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Err::<(), _>(TestError))
        }
    });
    tokio::pin!(future);

    let reached = tokio::select! {
        result = &mut future => {
            panic!("retry completed before manual time advanced: {result:?}");
        }
        reached = clock.advance_to_next_deadline_async() => reached,
    };
    assert_eq!(
        reached.elapsed_since_origin(),
        Duration::from_secs(1),
        "the flow deadline, not the full backoff, must drive the timer"
    );

    let error = future
        .await
        .expect_err("flow timeout should terminate retry");
    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn worker_facade_retries_with_cooperative_token() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let result = retry
        .worker()
        .run({
            let attempts = Arc::clone(&attempts);
            move |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(TestError)
                } else {
                    Ok(9_u32)
                }
            }
        })
        .expect("second worker attempt succeeds");
    assert_eq!(*result.value(), 9);
}

#[test]
fn worker_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let error = retry
        .worker()
        .attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::from_millis(50))
        .run(|token| {
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), _>(TestError)
        })
        .unwrap_err();

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Attempt
            }),
            ..
        }
    ));
}

#[test]
fn worker_shorter_flow_timeout_reports_flow_source() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let error = retry
        .worker()
        .attempt_timeout(Duration::from_secs(1))
        .flow_timeout(Duration::from_millis(10))
        .cancellation_grace(Duration::from_millis(50))
        .run(|token| {
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), _>(TestError)
        })
        .expect_err("flow timeout should terminate retry");

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Flow
            }),
            ..
        }
    ));
}

#[test]
fn worker_flow_timeout_caps_retry_sleep() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::from_millis(500)))
        .build()
        .unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let operation_attempts = Arc::clone(&attempts);
    let error = retry
        .worker()
        .flow_timeout(Duration::from_millis(10))
        .run(move |_| {
            operation_attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(TestError)
        })
        .expect_err("flow timeout should terminate retry");

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(
        error.context().total_elapsed() < Duration::from_millis(100),
        "flow timeout should cap the blocking sleep: {:?}",
        error.context().total_elapsed()
    );
}
