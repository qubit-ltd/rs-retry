// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use std::future;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;

use crate::support::AdvancingObserver;
use crate::support::FixedRetryRandomSource;
use crate::support::TestError;
use crate::support::retry_once_policy;

#[cfg(feature = "tokio")]
struct RegistrationAdvancingTimer {
    clock: Arc<ManualMonotonicClock>,
    timer: Arc<dyn Timer>,
    registered_deadline: mpsc::Sender<MonotonicInstant>,
    registrations: AtomicUsize,
}

#[cfg(feature = "tokio")]
impl Timer for RegistrationAdvancingTimer {
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        if self.registrations.fetch_add(1, Ordering::SeqCst) == 1 {
            self.registered_deadline
                .send(deadline)
                .expect("receiver remains available");
        }
        self.timer.at(deadline)
    }

    fn after(&self, duration: Duration) -> Result<TimerFuture, TimeError> {
        self.clock
            .advance(Duration::from_secs(6))
            .expect("advance registration clock");
        self.at(self.clock.now().checked_add(duration)?)
    }
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_async_backoff_registration_does_not_move_flow_deadline() {
    let clock = ManualMonotonicClock::new_shared();
    let (sender, receiver) = mpsc::channel();
    let timer: Arc<dyn Timer> = Arc::new(RegistrationAdvancingTimer {
        timer: clock.new_timer(),
        clock: Arc::clone(&clock),
        registered_deadline: sender,
        registrations: AtomicUsize::new(0),
    });
    let task = tokio::spawn(async move {
        let policy = RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_secs(20)))
            .build()
            .expect("valid retry policy");
        Retry::<TestError>::builder(policy)
            .build()
            .asynchronous()
            .timer(timer)
            .flow_timeout(Duration::from_secs(10))
            .run(|| async { Err::<(), _>(TestError("retry")) })
            .await
    });
    let deadline = tokio::task::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(1)))
        .await
        .expect("deadline receiver task")
        .expect("backoff registration");
    if deadline.elapsed_since_origin() != Duration::from_secs(10) {
        task.abort();
        let _ = task.await;
        assert_eq!(deadline.elapsed_since_origin(), Duration::from_secs(10));
        return;
    }
    clock.advance(Duration::from_secs(20)).expect("reach flow deadline");
    let error = task
        .await
        .expect("async retry task completes")
        .expect_err("flow timeout");
    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
}

#[cfg(feature = "tokio")]
struct SecondRegistrationFailsTimer {
    clock: Arc<ManualMonotonicClock>,
    registrations: AtomicUsize,
}

#[cfg(feature = "tokio")]
impl SecondRegistrationFailsTimer {
    fn new() -> Self {
        Self {
            clock: ManualMonotonicClock::new_shared(),
            registrations: AtomicUsize::new(0),
        }
    }
}

#[cfg(feature = "tokio")]
impl Timer for SecondRegistrationFailsTimer {
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    fn at(&self, _deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        if self.registrations.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(Box::pin(future::pending()))
        } else {
            Err(TimeError::InstantOverflow)
        }
    }
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_async_facade_reports_timer_failure_with_injected_components() {
    let timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let random = Arc::new(FixedRetryRandomSource::new(0.5));
    let error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .timer(timer)
        .random_source(random)
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(
        error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let attempts_exhausted = Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("only attempt")) })
        .await
        .unwrap_err();
    assert!(matches!(
        attempts_exhausted.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));

    let delay_rejected = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .max_total_elapsed(Duration::from_millis(1))
            .backoff(BackoffPolicy::fixed(Duration::from_secs(1)))
            .build()
            .unwrap(),
    )
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("retry")) })
    .await
    .unwrap_err();
    assert!(matches!(
        delay_rejected.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let aborted = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Abort)
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("fatal")) })
        .await
        .unwrap_err();
    assert!(matches!(aborted.failure(), RetryFailure::Aborted { .. }));

    let clock = ManualMonotonicClock::new_shared();
    let expired_by_observer = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_total_elapsed(Duration::from_secs(1))
            .build()
            .unwrap(),
    )
    .observer(AdvancingObserver(Arc::clone(&clock)))
    .build()
    .asynchronous()
    .timer(clock.new_timer())
    .run(|| async { Ok::<_, TestError>(()) })
    .await
    .unwrap_err();
    assert!(matches!(
        expired_by_observer.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let registration_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let attempt_registration_error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .attempt_timeout(Duration::from_secs(1))
        .timer(registration_timer)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        attempt_registration_error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let completion_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Completion,
        "retry-test",
        "offline",
    ));
    let attempt_completion_error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .attempt_timeout(Duration::from_secs(1))
        .timer(completion_timer)
        .run(future::pending::<Result<(), TestError>>)
        .await
        .unwrap_err();
    assert!(matches!(
        attempt_completion_error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let rule_panics = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| panic!("rule panic"))
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(rule_panics.failure(), RetryFailure::CallbackFailed { .. }));

    let zero_budget = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_total_elapsed(Duration::ZERO)
            .build()
            .unwrap(),
    )
    .build()
    .asynchronous()
    .run(|| async { Ok::<_, TestError>(()) })
    .await
    .unwrap_err();
    assert!(matches!(
        zero_budget.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let successful_timed_attempt = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .attempt_timeout(Duration::from_secs(1))
        .run(|| async { Ok::<_, TestError>(23_u32) })
        .await
        .unwrap();
    assert_eq!(*successful_timed_attempt.value(), 23);

    let tie = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .attempt_timeout(Duration::from_millis(1))
        .flow_timeout(Duration::from_millis(5))
        .run(future::pending::<Result<(), TestError>>)
        .await
        .unwrap_err();
    assert!(matches!(
        tie.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            last_failure: Some(AttemptFailure::TimedOut {
                scope: RetryTimeoutScope::Attempt
            }),
            ..
        }
    ));

    let cap_timer: Arc<dyn Timer> = Arc::new(SecondRegistrationFailsTimer::new());
    let cap_error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_secs(2)))
            .build()
            .unwrap(),
    )
    .build()
    .asynchronous()
    .flow_timeout(Duration::from_secs(1))
    .timer(cap_timer)
    .run(|| async { Err::<(), _>(TestError("retry")) })
    .await
    .unwrap_err();
    assert!(matches!(
        cap_error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let zero_flow = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .flow_timeout(Duration::ZERO)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        zero_flow.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let flow_expired_by_observer = Retry::<TestError>::builder(retry_once_policy())
        .observer(AdvancingObserver(Arc::clone(&clock)))
        .build()
        .asynchronous()
        .flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer())
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        flow_expired_by_observer.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));

    let attempts = AtomicUsize::new(0);
    let jittered_retry = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::RetryWithJitteredHint(Duration::ZERO))
        .build()
        .asynchronous()
        .run(|| async {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(29_u32)
            }
        })
        .await
        .expect("jittered hint retry should succeed");
    assert_eq!(*jittered_retry.value(), 29);
}
