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

use qubit_clock::ClockDomain;
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
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::AdvancingObserver;
use crate::support::FixedRetryRandomSource;
use crate::support::TestError;
use crate::support::retry_once_policy;

struct DefaultObserver;

impl RetryObserver<TestError> for DefaultObserver {}

#[test]
fn test_sync_facade_reports_timer_and_budget_boundaries() {
    let timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let random = Arc::new(FixedRetryRandomSource::new(0.5));
    let config = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .fallback(RetryFallback::Retry)
        .build().expect("valid config");
    let error = Retry::new(&config)
        .timer(timer)
        .random_source(random)
        .run(|| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_hard_attempt_timeout(), None);

    let exhausted_config = RetryConfig::<TestError>::builder()
        
                .operation_time_budget(Duration::ZERO)

        .build()
        .expect("valid config");
    let exhausted = Retry::new(&exhausted_config)
        .run(|| Ok::<_, TestError>(()))
        .unwrap_err();
    assert!(matches!(
        exhausted.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::OperationElapsed,
            ..
        }
    ));

    let config2 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Abort)
        .build().expect("valid config");
    let aborted = Retry::new(&config2)
        .run(|| Err::<(), _>(TestError("fatal")))
        .unwrap_err();
    assert!(matches!(aborted.reason(), RetryErrorReason::Aborted));

    let config3 = RetryConfig::<TestError>::builder().max_attempts(1)
        .fallback(RetryFallback::Retry)
        .build().expect("valid config");
    let attempts_exhausted = Retry::new(&config3)
        .run(|| Err::<(), _>(TestError("only attempt")))
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
    let delay_rejected = Retry::new(&delay_config)
        .run(|| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        delay_rejected.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let observer = AdvancingObserver(Arc::clone(&clock));
    let observer_config = RetryConfig::<TestError>::builder()
        
                .total_time_budget(Duration::from_secs(1))

        .observer(observer)
        .build()
        .expect("valid config");
    let expired_by_observer = Retry::new(&observer_config)
        .timer(clock.new_timer())
        .run(|| Ok::<_, TestError>(()))
        .unwrap_err();
    assert!(matches!(
        expired_by_observer.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let attempts = AtomicU32::new(0);
    let config4 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::RetryWithHint(Duration::ZERO))
        .observer(DefaultObserver)
        .build().expect("valid config");
    let hinted_retry = Retry::new(&config4)
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(17_u32)
            }
        })
        .unwrap();
    assert_eq!(*hinted_retry.value(), 17);

    let attempts = AtomicU32::new(0);
    let config5 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::RetryWithJitteredHint(Duration::ZERO))
        .build().expect("valid config");
    let jittered_retry = Retry::new(&config5)
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(19_u32)
            }
        })
        .expect("jittered hint retry should succeed");
    assert_eq!(*jittered_retry.value(), 19);

    let callback_count = Arc::new(AtomicU32::new(0));
    let callback_count_for_observer = Arc::clone(&callback_count);
    let observer_config = RetryConfig::<TestError>::builder()
        .max_attempts(1)
        .observer(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            callback_count_for_observer.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("valid config");
    let _ = Retry::new(&observer_config).run(|| Err::<(), _>(TestError("observed")));
    assert_eq!(callback_count.load(Ordering::SeqCst), 1);
}

/// Changes only the commitment sample, after the control callback refresh.
struct CommitRegressingClock {
    domain: ClockDomain,
    samples: AtomicU32,
}

impl MonotonicClock for CommitRegressingClock {
    fn domain(&self) -> ClockDomain {
        self.domain
    }
    fn now(&self) -> MonotonicInstant {
        let sample = self.samples.fetch_add(1, Ordering::SeqCst);
        MonotonicInstant::new(
            self.domain,
            if sample < 3 {
                Duration::from_secs(1)
            } else {
                Duration::ZERO
            },
        )
    }
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("the supplied timer already owns this scripted clock")
    }
}

impl Timer for CommitRegressingClock {
    fn clock(&self) -> &dyn MonotonicClock {
        self
    }
    fn at(&self, _: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        panic!("no timer should be registered before rejected synchronous admission")
    }
}

#[test]
fn test_sync_commit_revalidates_clock_before_counting_operation() {
    let config6 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .build().expect("valid config");
    let error = Retry::new(&config6)
        .timer(Arc::new(CommitRegressingClock {
            domain: ClockDomain::new(),
            samples: AtomicU32::new(0),
        }))
        .run(|| -> Result<(), TestError> { panic!("invalid commitment must not start user code") })
        .unwrap_err();
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().operation_elapsed(), Duration::ZERO);
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Clock { .. },
            ..
        }
    ));
}
