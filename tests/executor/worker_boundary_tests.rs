// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::Timer;
use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::WorkerRetry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

use crate::support::AdvancingObserver;
use crate::support::FixedRetryRandomSource;
use crate::support::TestError;
use crate::support::retry_once_policy;

#[test]
fn test_worker_facade_reports_timer_panic_and_detached_worker() {
    let timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let random = Arc::new(FixedRetryRandomSource::new(0.5));
    let config = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .fallback(RetryFallback::Retry)
        .build().expect("valid config");
    let timer_error = WorkerRetry::new(&config)
        .timer(timer)
        .random_source(random)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        timer_error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let config2 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .build().expect("valid config");
    let panic_error = WorkerRetry::new(&config2)
        .run(|_| -> Result<(), TestError> { panic!("isolated") })
        .unwrap_err();
    assert!(matches!(panic_error.reason(), RetryErrorReason::Aborted));

    let (release_sender, release_receiver) = mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let config3 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .build().expect("valid config");
    let detached = WorkerRetry::new(&config3)
        .timer(clock.new_timer())
        .hard_attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::from_millis(1))
        .run({
            let release_receiver = Arc::clone(&release_receiver);
            move |_| {
                operation_clock
                    .advance(Duration::from_millis(1))
                    .expect("expire admitted attempt");
                release_receiver.lock().unwrap().recv().unwrap();
                Ok::<_, TestError>(())
            }
        })
        .unwrap_err();
    release_sender.send(()).unwrap();
    assert!(matches!(
        detached.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::AttemptTimeout
            },
            ..
        }
    ));
    assert_eq!(detached.context().current_attempt().map(NonZeroU32::get), Some(1));
    assert_eq!(
        detached.context().current_hard_attempt_timeout(),
        Some(Duration::from_millis(1))
    );

    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let chain_config1 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .build().expect("valid config");
    let zero_grace = WorkerRetry::new(&chain_config1)
        .hard_attempt_timeout(Duration::from_millis(1))
        .timer(clock.new_timer())
        .cancellation_grace(Duration::ZERO)
        .run(move |token| {
            operation_clock
                .advance(Duration::from_millis(1))
                .expect("expire admitted attempt");
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Ok::<_, TestError>(())
        })
        .unwrap_err();
    assert!(matches!(
        zero_grace.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::AttemptTimeout,
            },
            ..
        } | RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            ..
        }
    ));

    let config4 = RetryConfig::<TestError>::builder().max_attempts(1)
        .fallback(RetryFallback::Retry)
        .build().expect("valid config");
    let attempts_exhausted = WorkerRetry::new(&config4)
        .run(|_| Err::<(), _>(TestError("only attempt")))
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
    let delay_rejected = WorkerRetry::new(&delay_config)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        delay_rejected.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let observer_config = RetryConfig::<TestError>::builder()
        
                .total_time_budget(Duration::from_secs(1))

        .observer(AdvancingObserver(Arc::clone(&clock)))
        .build()
        .expect("valid config");
    let expired_by_observer = WorkerRetry::new(&observer_config)
        .timer(clock.new_timer())
        .run(|_| Ok::<_, TestError>(()))
        .unwrap_err();
    assert!(matches!(
        expired_by_observer.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let config5 = RetryConfig::<TestError>::builder().policy(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| panic!("rule panic"))
        .build().expect("valid config");
    let rule_panics = WorkerRetry::new(&config5)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(rule_panics.reason(), RetryErrorReason::CallbackFailed { .. }));

    let zero_config = RetryConfig::<TestError>::builder()
        
                .total_time_budget(Duration::ZERO)

        .build()
        .expect("valid config");
    let zero_budget = WorkerRetry::new(&zero_config)
        .run(|_| Ok::<_, TestError>(()))
        .unwrap_err();
    assert!(matches!(
        zero_budget.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let cap_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let cap_config = RetryConfig::<TestError>::builder()
        
                .max_attempts(2)
                .backoff(BackoffPolicy::fixed(Duration::from_secs(2)))

        .build()
        .expect("valid config");
    let cap_error = WorkerRetry::new(&cap_config)
        .hard_flow_timeout(Duration::from_secs(1))
        .timer(cap_timer)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        cap_error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let config6 = RetryConfig::<TestError>::builder().max_attempts(1)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Retry)
        .build().expect("valid config");
    let explicit_retry = WorkerRetry::new(&config6)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        explicit_retry.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));
}

/// Covers worker thread naming through a successful worker attempt.
#[test]
fn test_worker_facade_accepts_thread_name() {
    let config = RetryConfig::<TestError>::builder().max_attempts(1)
        .build().expect("valid config");
    let result = WorkerRetry::new(&config)
        .thread_name("coverage-worker")
        .run(|_| Ok::<_, TestError>(thread::current().name().map(str::to_owned)))
        .expect("named worker should start");

    assert_eq!(result.value().as_deref(), Some("coverage-worker"));
}

#[test]
fn test_worker_deadline_overflow_does_not_admit_operation() {
    for flow_timeout in [false, true] {
        let clock = ManualMonotonicClock::new_shared();
        clock.advance(Duration::from_nanos(1)).expect("nonzero clock origin");
        let retry = RetryConfig::<TestError>::builder().policy(retry_once_policy()).build().expect("valid config");
        let execution = WorkerRetry::new(&retry).timer(clock.new_timer());
        let execution = if flow_timeout {
            execution.hard_flow_timeout(Duration::MAX)
        } else {
            execution.hard_attempt_timeout(Duration::MAX)
        };
        let error = execution
            .run(|_| -> Result<(), TestError> { panic!("overflow must reject admission before spawning user work") })
            .unwrap_err();
        assert_eq!(error.context().attempts(), 0);
        assert_eq!(error.context().current_attempt(), None);
        assert!(matches!(
            error.reason(),
            RetryErrorReason::Infrastructure {
                failure: RetryInfrastructureFailure::Clock { .. },
            }
        ));
    }
}
