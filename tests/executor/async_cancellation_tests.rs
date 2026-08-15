// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use std::future::Future;
#[cfg(feature = "tokio")]
use std::future::poll_fn;
#[cfg(feature = "tokio")]
use std::sync::Arc;
#[cfg(feature = "tokio")]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "tokio")]
use std::sync::atomic::Ordering;
#[cfg(feature = "tokio")]
use std::task::Context;
#[cfg(feature = "tokio")]
use std::task::Poll;
#[cfg(feature = "tokio")]
use std::task::Waker;
#[cfg(feature = "tokio")]
use std::time::Duration;

#[cfg(feature = "tokio")]
use qubit_clock::ManualMonotonicClock;
#[cfg(feature = "tokio")]
use qubit_clock::MonotonicClock;
#[cfg(feature = "tokio")]
use qubit_clock::MonotonicInstant;
#[cfg(feature = "tokio")]
use qubit_clock::TimeError;
#[cfg(feature = "tokio")]
use qubit_clock::Timer;
#[cfg(feature = "tokio")]
use qubit_clock::TimerFuture;
#[cfg(feature = "tokio")]
use qubit_retry::AttemptFailure;
#[cfg(feature = "tokio")]
use qubit_retry::BackoffPolicy;
#[cfg(feature = "tokio")]
use qubit_retry::BackoffStep;
#[cfg(feature = "tokio")]
use qubit_retry::Retry;
#[cfg(feature = "tokio")]
use qubit_retry::RetryCancellationPhase;
#[cfg(feature = "tokio")]
use qubit_retry::RetryCancellationToken;
#[cfg(feature = "tokio")]
use qubit_retry::RetryContext;
#[cfg(feature = "tokio")]
use qubit_retry::RetryDecision;
#[cfg(feature = "tokio")]
use qubit_retry::RetryError;
#[cfg(feature = "tokio")]
use qubit_retry::RetryFailure;
#[cfg(feature = "tokio")]
use qubit_retry::RetryObserver;
#[cfg(feature = "tokio")]
use qubit_retry::RetryPolicy;

#[cfg(feature = "tokio")]
use crate::support::TestError;

/// Observer that cancels a flow from the attempt-started callback.
#[cfg(feature = "tokio")]
struct CancelOnAttemptStarted {
    token: RetryCancellationToken,
}

#[cfg(feature = "tokio")]
impl RetryObserver<TestError> for CancelOnAttemptStarted {
    /// Cancels the flow before the operation is committed.
    fn on_attempt_started(&self, _context: &RetryContext) {
        self.token.cancel();
    }
}

/// Observer that cancels a flow from the attempt-failed callback and records
/// whether scheduling continued afterward.
#[cfg(feature = "tokio")]
struct CancelOnAttemptFailed {
    token: RetryCancellationToken,
    scheduled_calls: Arc<AtomicUsize>,
}

/// Observer that cancels a flow from the retry-scheduled callback.
#[cfg(feature = "tokio")]
struct CancelOnRetryScheduled {
    token: RetryCancellationToken,
}

#[cfg(feature = "tokio")]
impl RetryObserver<TestError> for CancelOnRetryScheduled {
    /// Cancels after the controller has selected the next retry delay.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.token.cancel();
    }
}

/// Observer that counts retry-scheduled callbacks.
#[cfg(feature = "tokio")]
struct CountRetryScheduled {
    calls: Arc<AtomicUsize>,
}

/// Timer that cancels the retry while registering a backoff deadline and then
/// reports a deterministic registration failure.
#[cfg(feature = "tokio")]
struct CancellingFailingRegistrationTimer {
    clock: Arc<ManualMonotonicClock>,
    token: RetryCancellationToken,
    registrations: Arc<AtomicUsize>,
}

#[cfg(feature = "tokio")]
impl Timer for CancellingFailingRegistrationTimer {
    /// Returns the manual clock used to derive relative deadlines.
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    /// Cancels the retry during registration and returns a fixed failure.
    fn at(
        &self,
        _deadline: MonotonicInstant,
    ) -> Result<TimerFuture, TimeError> {
        self.registrations.fetch_add(1, Ordering::SeqCst);
        self.token.cancel();
        Err(TimeError::InstantOverflow)
    }
}

#[cfg(feature = "tokio")]
impl RetryObserver<TestError> for CountRetryScheduled {
    /// Records one retry-scheduled callback.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(feature = "tokio")]
impl RetryObserver<TestError> for CancelOnAttemptFailed {
    /// Cancels immediately after the first failed operation.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        self.token.cancel();
    }

    /// Records retry-scheduled callbacks that should be suppressed.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.scheduled_calls.fetch_add(1, Ordering::SeqCst);
    }
}

/// Asserts one cancellation terminal and returns its retained last failure.
#[cfg(feature = "tokio")]
fn assert_cancelled(
    error: &RetryError<TestError>,
    expected_phase: RetryCancellationPhase,
) -> Option<&AttemptFailure<TestError>> {
    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!(
            "expected a cancellation terminal, got {:?}",
            error.failure()
        );
    };
    assert_eq!(*phase, expected_phase);
    assert_eq!(error.failure().last_failure(), last_failure.as_ref());
    assert_eq!(
        error.failure().last_error(),
        last_failure.as_ref().and_then(AttemptFailure::as_error)
    );
    let suffix = last_failure
        .as_ref()
        .map(|failure| format!("; last attempt failed: {failure}"))
        .unwrap_or_default();
    assert_eq!(
        error.failure().to_string(),
        format!("retry cancelled: {expected_phase}{suffix}")
    );
    last_failure.as_ref()
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_cancellation_token_before_attempt_does_not_construct_operation() {
    let token = RetryCancellationToken::new();
    token.cancel();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("pre-cancellation policy should be valid"),
    )
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), TestError>(()) }
        }
    })
    .await
    .expect_err("pre-cancellation must stop before constructing an operation");

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::BeforeAttempt),
        None
    );
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(operation_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_cancellation_token_during_attempt_retains_active_attempt_scope() {
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let operation_polls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("attempt cancellation policy should be valid"),
    )
    .build()
    .asynchronous()
    .attempt_timeout(Duration::from_secs(5))
    .cancellation_token(token)
    .run({
        let operation_polls = Arc::clone(&operation_polls);
        move || {
            let operation_polls = Arc::clone(&operation_polls);
            let operation_token = operation_token.clone();
            poll_fn(move |_| {
                operation_polls.fetch_add(1, Ordering::SeqCst);
                operation_token.cancel();
                Poll::Pending::<Result<(), TestError>>
            })
        }
    })
    .await
    .expect_err("attempt cancellation must interrupt a pending operation");

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Attempt),
        None
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(
        error
            .context()
            .current_attempt()
            .map(|attempt| attempt.get()),
        Some(1)
    );
    assert_eq!(
        error.context().current_attempt_timeout(),
        Some(Duration::from_secs(5))
    );
    assert_eq!(operation_polls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_attempt_success_wins_when_cancellation_is_ready_in_same_poll() {
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let result = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("attempt race policy should be valid"),
    )
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run(move || {
        operation_token.cancel();
        std::future::ready(Ok::<_, TestError>("completed"))
    })
    .await
    .expect("operation success must win the same-poll cancellation race");

    assert_eq!(result.value(), &"completed");
    assert_eq!(result.context().attempts(), 1);
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_backoff_cancellation_wins_when_timer_is_ready_in_same_poll() {
    let delay = Duration::from_secs(2);
    let token = RetryCancellationToken::new();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(
                BackoffPolicy::fixed(Duration::from_secs(1))
                    .prefer_retry_after(),
            )
            .build()
            .expect("backoff cancellation policy should be valid"),
    )
    .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::RetryWithHint(delay)
    })
    .build();
    let executor = retry.asynchronous().cancellation_token(token.clone());
    let future = executor.run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TestError("backoff")) }
        }
    });
    tokio::pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(future.as_mut().poll(&mut context).is_pending());

    token.cancel();
    tokio::time::advance(delay).await;
    let error = match future.as_mut().poll(&mut context) {
        Poll::Ready(Err(error)) => error,
        Poll::Ready(Ok(_)) => panic!("same-poll backoff race must not retry"),
        Poll::Pending => panic!("ready cancellation must finish the retry"),
    };

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Backoff),
        Some(&AttemptFailure::Error(TestError("backoff")))
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().next_delay(), Some(delay));
    assert_eq!(error.context().retry_after_hint(), Some(delay));
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_attempt_started_callback_cancellation_stops_before_operation() {
    let token = RetryCancellationToken::new();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let rule_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("started-callback cancellation policy should be valid"),
    )
    .observer(CancelOnAttemptStarted {
        token: token.clone(),
    })
    .rule({
        let rule_calls = Arc::clone(&rule_calls);
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            rule_calls.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        }
    })
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TestError("must not run")) }
        }
    })
    .await
    .expect_err("callback cancellation must be rechecked before operation");

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::BeforeAttempt),
        None
    );
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(operation_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_attempt_failed_callback_cancellation_stops_before_rule() {
    let token = RetryCancellationToken::new();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let rule_calls = Arc::new(AtomicUsize::new(0));
    let scheduled_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("failed-callback cancellation policy should be valid"),
    )
    .observer(CancelOnAttemptFailed {
        token: token.clone(),
        scheduled_calls: Arc::clone(&scheduled_calls),
    })
    .rule({
        let rule_calls = Arc::clone(&rule_calls);
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            rule_calls.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        }
    })
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TestError("callback")) }
        }
    })
    .await
    .expect_err(
        "failed-callback cancellation must stop before rule evaluation",
    );

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Backoff),
        Some(&AttemptFailure::Error(TestError("callback")))
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
    assert_eq!(scheduled_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_rule_callback_cancellation_stops_before_retry_scheduling() {
    let token = RetryCancellationToken::new();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let scheduled_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("rule-callback cancellation policy should be valid"),
    )
    .rule({
        let token = token.clone();
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            token.cancel();
            RetryDecision::Retry
        }
    })
    .observer(CountRetryScheduled {
        calls: Arc::clone(&scheduled_calls),
    })
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TestError("rule")) }
        }
    })
    .await
    .expect_err("rule cancellation must be rechecked before scheduling");

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Backoff),
        Some(&AttemptFailure::Error(TestError("rule")))
    );
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
    assert_eq!(scheduled_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_retry_scheduled_callback_cancellation_stops_before_sleep() {
    let delay = Duration::from_secs(3);
    let token = RetryCancellationToken::new();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(delay))
            .build()
            .expect("scheduled-callback cancellation policy should be valid"),
    )
    .observer(CancelOnRetryScheduled {
        token: token.clone(),
    })
    .build()
    .asynchronous()
    .cancellation_token(token)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TestError("scheduled")) }
        }
    })
    .await
    .expect_err("scheduled callback cancellation must stop before sleeping");

    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Backoff),
        Some(&AttemptFailure::Error(TestError("scheduled")))
    );
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().next_delay(), Some(delay));
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_backoff_registration_cancellation_wins_over_timer_failure() {
    let delay = Duration::from_secs(4);
    let token = RetryCancellationToken::new();
    let registrations = Arc::new(AtomicUsize::new(0));
    let timer = Arc::new(CancellingFailingRegistrationTimer {
        clock: ManualMonotonicClock::new_shared(),
        token: token.clone(),
        registrations: Arc::clone(&registrations),
    });
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(
                BackoffPolicy::fixed(Duration::from_secs(1))
                    .prefer_retry_after(),
            )
            .build()
            .expect("registration cancellation policy should be valid"),
    )
    .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::RetryWithHint(delay)
    })
    .build()
    .asynchronous()
    .timer(timer)
    .cancellation_token(token)
    .run(|| async { Err::<(), _>(TestError("registration")) })
    .await
    .expect_err("registration-time cancellation must stop the retry");

    assert_eq!(registrations.load(Ordering::SeqCst), 1);
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().next_delay(), Some(delay));
    assert_eq!(error.context().retry_after_hint(), Some(delay));
    assert_eq!(
        assert_cancelled(&error, RetryCancellationPhase::Backoff),
        Some(&AttemptFailure::Error(TestError("registration")))
    );
}
