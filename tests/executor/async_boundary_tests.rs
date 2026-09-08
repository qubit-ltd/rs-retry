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
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::TokioRetry;

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
        let config = RetryConfig::<TestError>::builder()
            .policy(policy)
            .fallback(RetryFallback::Retry)
            .build()
            .expect("valid config");
        TokioRetry::new(&config)
            .timer(timer)
            .hard_flow_timeout(Duration::from_secs(10))
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
        error.reason(),
        RetryErrorReason::TimedOut {
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
    let config2 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let error = TokioRetry::new(&config2)
        .timer(timer)
        .random_source(random)
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let config3 = RetryConfig::<TestError>::builder()
        .max_attempts(1)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let attempts_exhausted = TokioRetry::new(&config3)
        .run(|| async { Err::<(), _>(TestError("only attempt")) })
        .await
        .unwrap_err();
    assert!(matches!(
        attempts_exhausted.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));

    let delay_config = RetryConfig::<TestError>::builder()
        .max_attempts(2)
        .total_time_budget(Duration::from_millis(1))
        .backoff(BackoffPolicy::fixed(Duration::from_secs(1)))
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let delay_rejected = TokioRetry::new(&delay_config)
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(
        delay_rejected.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let config4 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Abort)
        .build()
        .expect("valid config");
    let aborted = TokioRetry::new(&config4)
        .run(|| async { Err::<(), _>(TestError("fatal")) })
        .await
        .unwrap_err();
    assert!(matches!(aborted.reason(), RetryErrorReason::Aborted));

    let clock = ManualMonotonicClock::new_shared();
    let observer_config = RetryConfig::<TestError>::builder()
        .total_time_budget(Duration::from_secs(1))
        .observer(AdvancingObserver(Arc::clone(&clock)))
        .build()
        .expect("valid config");
    let expired_by_observer = TokioRetry::new(&observer_config)
        .timer(clock.new_timer())
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        expired_by_observer.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let registration_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let chain_config1 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .build()
        .expect("valid config");
    let attempt_registration_error = TokioRetry::new(&chain_config1)
        .hard_attempt_timeout(Duration::from_secs(1))
        .timer(registration_timer)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        attempt_registration_error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let completion_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Completion,
        "retry-test",
        "offline",
    ));
    let chain_config2 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .build()
        .expect("valid config");
    let attempt_completion_error = TokioRetry::new(&chain_config2)
        .hard_attempt_timeout(Duration::from_secs(1))
        .timer(completion_timer)
        .run(future::pending::<Result<(), TestError>>)
        .await
        .unwrap_err();
    assert!(matches!(
        attempt_completion_error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let config5 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| panic!("rule panic"))
        .build()
        .expect("valid config");
    let rule_panics = TokioRetry::new(&config5)
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(rule_panics.reason(), RetryErrorReason::CallbackFailed { .. }));

    let zero_config = RetryConfig::<TestError>::builder()
        .total_time_budget(Duration::ZERO)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let zero_budget = TokioRetry::new(&zero_config)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        zero_budget.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let chain_config3 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .build()
        .expect("valid config");
    let successful_timed_attempt = TokioRetry::new(&chain_config3)
        .hard_attempt_timeout(Duration::from_secs(1))
        .run(|| async { Ok::<_, TestError>(23_u32) })
        .await
        .unwrap();
    assert_eq!(*successful_timed_attempt.value(), 23);

    let chain_config4 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .build()
        .expect("valid config");
    let tie = TokioRetry::new(&chain_config4)
        .hard_attempt_timeout(Duration::from_millis(1))
        .hard_flow_timeout(Duration::from_millis(5))
        .run(future::pending::<Result<(), TestError>>)
        .await
        .unwrap_err();
    assert!(matches!(
        tie.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        }
    ));

    let cap_timer: Arc<dyn Timer> = Arc::new(SecondRegistrationFailsTimer::new());
    let cap_config = RetryConfig::<TestError>::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::from_secs(2)))
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let cap_error = TokioRetry::new(&cap_config)
        .hard_flow_timeout(Duration::from_secs(1))
        .timer(cap_timer)
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(
        cap_error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let config6 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .build()
        .expect("valid config");
    let zero_flow = TokioRetry::new(&config6)
        .hard_flow_timeout(Duration::ZERO)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        zero_flow.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let chain_config5 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .observer(AdvancingObserver(Arc::clone(&clock)))
        .build()
        .expect("valid config");
    let flow_expired_by_observer = TokioRetry::new(&chain_config5)
        .hard_flow_timeout(Duration::from_secs(1))
        .timer(clock.new_timer())
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert!(matches!(
        flow_expired_by_observer.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));

    let attempts = AtomicUsize::new(0);
    let config7 = RetryConfig::<TestError>::builder()
        .policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::RetryWithJitteredHint(Duration::ZERO))
        .build()
        .expect("valid config");
    let jittered_retry = TokioRetry::new(&config7)
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
