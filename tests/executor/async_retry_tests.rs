// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use std::future::pending;
#[cfg(feature = "tokio")]
use std::future::poll_fn;
#[cfg(feature = "tokio")]
use std::num::NonZeroU32;
#[cfg(feature = "tokio")]
use std::sync::Arc;
#[cfg(feature = "tokio")]
use std::sync::Mutex;
#[cfg(feature = "tokio")]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "tokio")]
use std::sync::atomic::Ordering;
#[cfg(feature = "tokio")]
use std::task::Poll;
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
use qubit_clock::test_util::FaultInjectingTimer;
#[cfg(feature = "tokio")]
use qubit_clock::test_util::TimerFailurePoint;
#[cfg(feature = "tokio")]
use qubit_retry::AttemptFailure;
#[cfg(feature = "tokio")]
use qubit_retry::BackoffPolicy;
#[cfg(feature = "tokio")]
use qubit_retry::Retry;
#[cfg(feature = "tokio")]
use qubit_retry::RetryCallbackPhase;
#[cfg(feature = "tokio")]
use qubit_retry::RetryContext;
#[cfg(feature = "tokio")]
use qubit_retry::RetryDecision;
#[cfg(feature = "tokio")]
use qubit_retry::RetryFailure;
#[cfg(feature = "tokio")]
use qubit_retry::RetryInfrastructureFailure;
#[cfg(feature = "tokio")]
use qubit_retry::RetryLimitKind;
#[cfg(feature = "tokio")]
use qubit_retry::RetryPolicy;
#[cfg(feature = "tokio")]
use qubit_retry::RetryTimeoutScope;

#[cfg(feature = "tokio")]
use crate::support::CountingPhaseObserver;
#[cfg(feature = "tokio")]
use crate::support::ElapsedObserverCallback;
#[cfg(feature = "tokio")]
use crate::support::ElapsedRuleCallback;
#[cfg(feature = "tokio")]
use crate::support::ObserverPhaseCounts;
#[cfg(feature = "tokio")]
use crate::support::PanickingPhaseObserver;
#[cfg(feature = "tokio")]
use crate::support::TestError;
#[cfg(feature = "tokio")]
use crate::support::assert_callback_panic_elapsed;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_abort;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_infrastructure;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_limit;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_observer_panic;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_rule_panic;
#[cfg(feature = "tokio")]
use crate::support::assert_matrix_timeout;
#[cfg(feature = "tokio")]
use crate::support::callback_elapsed_records;
#[cfg(feature = "tokio")]
use crate::support::completion_regressing_timer;

/// Timer fixture that advances manual time while registering an absolute
/// deadline and records the exact deadline supplied by the facade.
#[cfg(feature = "tokio")]
struct AdvancingAtTimer {
    clock: Arc<ManualMonotonicClock>,
    registration_advance: Duration,
    deadline: Mutex<Option<MonotonicInstant>>,
}

#[cfg(feature = "tokio")]
impl AdvancingAtTimer {
    /// Creates a timer that advances by `registration_advance` in
    /// [`Timer::at`].
    fn new(
        clock: Arc<ManualMonotonicClock>,
        registration_advance: Duration,
    ) -> Self {
        Self {
            clock,
            registration_advance,
            deadline: Mutex::new(None),
        }
    }

    /// Returns the one absolute deadline observed during registration.
    fn deadline(&self) -> MonotonicInstant {
        self.deadline
            .lock()
            .expect("recorded deadline mutex should not be poisoned")
            .expect("the async facade should register one deadline")
    }
}

#[cfg(feature = "tokio")]
impl Timer for AdvancingAtTimer {
    /// Returns the manual clock advanced during registration.
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    /// Records the fixed deadline, advances time, and registers it unchanged.
    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        *self
            .deadline
            .lock()
            .expect("recorded deadline mutex should not be poisoned") =
            Some(deadline);
        self.clock.advance(self.registration_advance)?;
        self.clock.new_timer().at(deadline)
    }
}

#[cfg(feature = "tokio")]
#[test]
fn async_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.asynchronous();
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_retry_matches_shared_terminal_matrix() {
    let abort = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Abort
    })
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("matrix")) })
    .await
    .expect_err("the explicit abort rule must terminate after attempt one");
    assert_matrix_abort(&abort);

    let attempts = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("matrix")) })
    .await
    .expect_err("one admitted failure must exhaust the attempt limit");
    assert_matrix_limit(&attempts, RetryLimitKind::Attempts, 1, true);

    for limit in [
        RetryLimitKind::OperationElapsed,
        RetryLimitKind::TotalElapsed,
    ] {
        let clock = ManualMonotonicClock::new_shared();
        let mut policy = RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::immediate());
        policy = match limit {
            RetryLimitKind::OperationElapsed => {
                policy.max_operation_elapsed(Duration::from_secs(1))
            }
            RetryLimitKind::TotalElapsed => {
                policy.max_total_elapsed(Duration::from_secs(1))
            }
            RetryLimitKind::Attempts => unreachable!(),
        };
        let error = Retry::<TestError>::builder(policy.build().unwrap())
            .build()
            .asynchronous()
            .timer(clock.new_timer())
            .run(|| {
                clock
                    .advance(Duration::from_secs(1))
                    .expect("manual matrix clock should advance");
                async { Err::<(), _>(TestError("matrix")) }
            })
            .await
            .expect_err("the elapsed limit must reject continuation");
        assert_matrix_limit(&error, limit, 1, true);
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_retry_matches_shared_callback_matrix() {
    let later_rule_calls = Arc::new(AtomicUsize::new(0));
    let rule_error = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        panic!("matrix rule panic")
    })
    .rule({
        let later_rule_calls = Arc::clone(&later_rule_calls);
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            later_rule_calls.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        }
    })
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("matrix")) })
    .await
    .expect_err("the first panicking rule must fail closed");
    assert_matrix_rule_panic(&rule_error, later_rule_calls.as_ref());

    for phase in [
        RetryCallbackPhase::AttemptStarted,
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RetryScheduled,
    ] {
        let later_counts = Arc::new(ObserverPhaseCounts::default());
        let error = Retry::<TestError>::builder(
            RetryPolicy::builder()
                .max_attempts(2)
                .backoff(BackoffPolicy::immediate())
                .build()
                .unwrap(),
        )
        .observer(PanickingPhaseObserver::new(phase))
        .observer(CountingPhaseObserver(Arc::clone(&later_counts)))
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("matrix")) })
        .await
        .expect_err("the first panicking observer must fail closed");
        assert_matrix_observer_panic(&error, phase, later_counts.as_ref());
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_retry_refreshes_elapsed_time_between_callback_phases() {
    let clock = ManualMonotonicClock::new_shared();
    let records = callback_elapsed_records();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .max_total_elapsed(Duration::from_secs(3))
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("callback elapsed policy should be valid");
    let error = Retry::<TestError>::builder(policy)
        .observer(ElapsedObserverCallback::new(
            Arc::clone(&clock),
            RetryCallbackPhase::AttemptFailed,
            Arc::clone(&records),
            false,
        ))
        .rule(ElapsedRuleCallback::new(
            Arc::clone(&clock),
            Arc::clone(&records),
            false,
        ))
        .observer(ElapsedObserverCallback::new(
            Arc::clone(&clock),
            RetryCallbackPhase::RetryScheduled,
            Arc::clone(&records),
            false,
        ))
        .build()
        .asynchronous()
        .timer(clock.new_timer())
        .run(|| async { Err::<(), _>(TestError("elapsed")) })
        .await
        .expect_err("scheduled callback time should exhaust the flow");

    assert_eq!(
        *records
            .lock()
            .expect("callback elapsed records should not be poisoned"),
        vec![
            (RetryCallbackPhase::AttemptFailed, Duration::ZERO),
            (RetryCallbackPhase::RuleDecision, Duration::from_secs(1)),
            (RetryCallbackPhase::RetryScheduled, Duration::from_secs(2)),
        ]
    );
    assert!(matches!(
        error.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(3));
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_retry_refreshes_elapsed_time_after_callback_panics() {
    for phase in [
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RuleDecision,
        RetryCallbackPhase::RetryScheduled,
    ] {
        let clock = ManualMonotonicClock::new_shared();
        let records = callback_elapsed_records();
        let policy = RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("callback panic policy should be valid");
        let error = if phase == RetryCallbackPhase::RuleDecision {
            Retry::<TestError>::builder(policy)
                .rule(ElapsedRuleCallback::new(
                    Arc::clone(&clock),
                    records,
                    true,
                ))
                .build()
                .asynchronous()
                .timer(clock.new_timer())
                .run(|| async { Err::<(), _>(TestError("elapsed")) })
                .await
        } else {
            Retry::<TestError>::builder(policy)
                .observer(ElapsedObserverCallback::new(
                    Arc::clone(&clock),
                    phase,
                    records,
                    true,
                ))
                .build()
                .asynchronous()
                .timer(clock.new_timer())
                .run(|| async { Err::<(), _>(TestError("elapsed")) })
                .await
        }
        .expect_err("the advancing callback should panic");
        assert_callback_panic_elapsed(&error, phase);
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_retry_matches_shared_infrastructure_and_timeout_matrix() {
    let timer_error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_millis(1)))
            .build()
            .unwrap(),
    )
    .build()
    .asynchronous()
    .timer(Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "matrix",
        "offline",
    )))
    .run(|| async { Err::<(), _>(TestError("matrix")) })
    .await
    .expect_err("retry sleep registration failure must be terminal");
    assert_matrix_infrastructure(&timer_error, "timer", 1, None, true);

    let clock_error =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .asynchronous()
            .timer(completion_regressing_timer())
            .run(|| async { Ok::<_, TestError>(()) })
            .await
            .expect_err("completion clock regression must be terminal");
    assert_matrix_infrastructure(&clock_error, "clock", 1, None, false);

    let attempt_timeout =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .asynchronous()
            .attempt_timeout(Duration::from_millis(1))
            .run(pending::<Result<(), TestError>>)
            .await
            .expect_err("the pending attempt must hit its attempt timeout");
    assert_matrix_timeout(&attempt_timeout, RetryTimeoutScope::Attempt, 1);

    let flow_timeout =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .asynchronous()
            .flow_timeout(Duration::from_millis(1))
            .run(pending::<Result<(), TestError>>)
            .await
            .expect_err("the pending attempt must hit its flow timeout");
    assert_matrix_timeout(&flow_timeout, RetryTimeoutScope::Flow, 1);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_timeout_registration_failure_does_not_start_attempt() {
    let poll_count = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("registration failure policy should be valid"),
    )
    .build()
    .asynchronous()
    .attempt_timeout(Duration::from_secs(1))
    .timer(Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "attempt-timeout",
        "offline",
    )))
    .run({
        let poll_count = Arc::clone(&poll_count);
        move || {
            let poll_count = Arc::clone(&poll_count);
            poll_fn(move |_| {
                poll_count.fetch_add(1, Ordering::SeqCst);
                Poll::Ready(Ok::<(), TestError>(()))
            })
        }
    })
    .await
    .expect_err("timeout registration failure must stop before the attempt");

    assert!(matches!(
        error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            last_failure: None,
            ..
        }
    ));
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_attempt_timeout(), None);
    assert_eq!(poll_count.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_timeout_uses_fixed_deadline_and_preserves_selected_scope() {
    for (attempt_timeout, flow_timeout, expected_scope) in [
        (
            Duration::from_secs(10),
            Duration::from_secs(5),
            RetryTimeoutScope::Flow,
        ),
        (
            Duration::from_secs(5),
            Duration::from_secs(10),
            RetryTimeoutScope::Attempt,
        ),
        (
            Duration::from_secs(5),
            Duration::from_secs(5),
            RetryTimeoutScope::Attempt,
        ),
    ] {
        let clock = ManualMonotonicClock::new_shared();
        let flow_started_at = clock.now();
        let flow_deadline = flow_started_at
            .checked_add(flow_timeout)
            .expect("test flow deadline should be representable");
        let expected_deadline = flow_started_at
            .checked_add(attempt_timeout.min(flow_timeout))
            .expect("test effective deadline should be representable");
        let timer = Arc::new(AdvancingAtTimer::new(
            Arc::clone(&clock),
            Duration::from_secs(1),
        ));
        let operation_polls = Arc::new(AtomicUsize::new(0));
        let operation_advance =
            attempt_timeout.min(flow_timeout) - Duration::from_secs(1);
        let error = Retry::<TestError>::builder(
            RetryPolicy::builder()
                .max_attempts(1)
                .build()
                .expect("absolute deadline policy should be valid"),
        )
        .build()
        .asynchronous()
        .attempt_timeout(attempt_timeout)
        .flow_timeout(flow_timeout)
        .timer(timer.clone())
        .run({
            let clock = Arc::clone(&clock);
            let operation_polls = Arc::clone(&operation_polls);
            move || {
                let clock = Arc::clone(&clock);
                let operation_polls = Arc::clone(&operation_polls);
                poll_fn(move |_| {
                    if operation_polls.fetch_add(1, Ordering::SeqCst) == 0 {
                        clock
                            .advance(operation_advance)
                            .expect("operation should advance to the deadline");
                    }
                    Poll::Pending::<Result<(), TestError>>
                })
            }
        })
        .await
        .expect_err("the fixed absolute deadline should time out");

        let recorded_deadline = timer.deadline();
        assert_eq!(recorded_deadline, expected_deadline);
        assert!(
            recorded_deadline.elapsed_since_origin()
                <= flow_deadline.elapsed_since_origin()
        );
        let RetryFailure::TimedOut {
            scope,
            last_failure,
            ..
        } = error.failure()
        else {
            panic!("expected the prepared hard timeout to terminate the flow");
        };
        assert_eq!(*scope, expected_scope);
        assert_eq!(
            last_failure,
            &Some(AttemptFailure::TimedOut {
                scope: expected_scope,
            })
        );
        assert_eq!(error.context().attempts(), 1);
        assert!(operation_polls.load(Ordering::SeqCst) >= 1);
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_registration_reaching_deadline_does_not_start_operation() {
    let clock = ManualMonotonicClock::new_shared();
    let timer = Arc::new(AdvancingAtTimer::new(
        Arc::clone(&clock),
        Duration::from_secs(1),
    ));
    let poll_count = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("deadline admission policy should be valid"),
    )
    .build()
    .asynchronous()
    .attempt_timeout(Duration::from_secs(1))
    .timer(timer)
    .run({
        let poll_count = Arc::clone(&poll_count);
        move || {
            let poll_count = Arc::clone(&poll_count);
            poll_fn(move |_| {
                poll_count.fetch_add(1, Ordering::SeqCst);
                Poll::Ready(Ok::<(), TestError>(()))
            })
        }
    })
    .await
    .expect_err("a deadline reached during registration must stop admission");

    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            last_failure: None,
            ..
        }
    ));
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_attempt_timeout(), None);
    assert_eq!(poll_count.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_timeout_polling_failure_retains_active_attempt_scope() {
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(1)
            .build()
            .expect("timer polling failure policy should be valid"),
    )
    .build()
    .asynchronous()
    .attempt_timeout(Duration::from_secs(1))
    .timer(Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Completion,
        "attempt-timeout",
        "offline",
    )))
    .run(pending::<Result<(), TestError>>)
    .await
    .expect_err("timer polling failure must retain the active attempt");

    assert!(matches!(
        error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            last_failure: None,
            ..
        }
    ));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(
        error.context().current_attempt().map(NonZeroU32::get),
        Some(1)
    );
    assert_eq!(
        error.context().current_attempt_timeout(),
        Some(Duration::from_secs(1))
    );
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_success_counts_one_started_attempt_with_or_without_timeout() {
    let without_timeout =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .asynchronous()
            .run(|| async { Ok::<_, TestError>(()) })
            .await
            .expect("an immediate operation without a timeout should succeed");
    assert_eq!(without_timeout.context().attempts(), 1);

    let with_timeout =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .asynchronous()
            .attempt_timeout(Duration::from_secs(1))
            .run(|| async { Ok::<_, TestError>(()) })
            .await
            .expect("an immediate operation with a timeout should succeed");
    assert_eq!(with_timeout.context().attempts(), 1);
}
