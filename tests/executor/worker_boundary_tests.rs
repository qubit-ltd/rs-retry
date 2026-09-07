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
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
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
    let timer_error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .timer(timer)
        .random_source(random)
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        timer_error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let panic_error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .run(|_| -> Result<(), TestError> { panic!("isolated") })
        .unwrap_err();
    assert!(matches!(
        panic_error.failure(),
        RetryFailure::Aborted {
            last_failure: AttemptFailure::Panicked { .. },
            ..
        }
    ));

    let (release_sender, release_receiver) = mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let detached = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .timer(clock.new_timer())
        .attempt_timeout(Duration::from_millis(1))
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
        detached.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::AttemptTimeout
            },
            ..
        }
    ));
    assert_eq!(detached.context().current_attempt().map(NonZeroU32::get), Some(1));
    assert_eq!(
        detached.context().current_attempt_timeout(),
        Some(Duration::from_millis(1))
    );

    let clock = ManualMonotonicClock::new_shared();
    let operation_clock = Arc::clone(&clock);
    let zero_grace = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .attempt_timeout(Duration::from_millis(1))
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
        zero_grace.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::AttemptTimeout,
            },
            ..
        } | RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
            ..
        }
    ));

    let attempts_exhausted = Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .build()
        .worker()
        .run(|_| Err::<(), _>(TestError("only attempt")))
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
    .worker()
    .run(|_| Err::<(), _>(TestError("retry")))
    .unwrap_err();
    assert!(matches!(
        delay_rejected.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let expired_by_observer = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_total_elapsed(Duration::from_secs(1))
            .build()
            .unwrap(),
    )
    .observer(AdvancingObserver(Arc::clone(&clock)))
    .build()
    .worker()
    .timer(clock.new_timer())
    .run(|_| Ok::<_, TestError>(()))
    .unwrap_err();
    assert!(matches!(
        expired_by_observer.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let rule_panics = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| panic!("rule panic"))
        .build()
        .worker()
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(rule_panics.failure(), RetryFailure::CallbackFailed { .. }));

    let zero_budget = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_total_elapsed(Duration::ZERO)
            .build()
            .unwrap(),
    )
    .build()
    .worker()
    .run(|_| Ok::<_, TestError>(()))
    .unwrap_err();
    assert!(matches!(
        zero_budget.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let cap_timer: Arc<dyn Timer> = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "retry-test",
        "offline",
    ));
    let cap_error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_secs(2)))
            .build()
            .unwrap(),
    )
    .build()
    .worker()
    .flow_timeout(Duration::from_secs(1))
    .timer(cap_timer)
    .run(|_| Err::<(), _>(TestError("retry")))
    .unwrap_err();
    assert!(matches!(
        cap_error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));

    let explicit_retry = Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Retry)
        .build()
        .worker()
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        explicit_retry.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));
}

/// Covers worker thread naming through a successful worker attempt.
#[test]
fn test_worker_facade_accepts_thread_name() {
    let result = Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .build()
        .worker()
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
        let retry = Retry::<TestError>::builder(retry_once_policy()).build();
        let execution = retry.worker().timer(clock.new_timer());
        let execution = if flow_timeout {
            execution.flow_timeout(Duration::MAX)
        } else {
            execution.attempt_timeout(Duration::MAX)
        };
        let error = execution
            .run(|_| -> Result<(), TestError> { panic!("overflow must reject admission before spawning user work") })
            .unwrap_err();
        assert_eq!(error.context().attempts(), 0);
        assert_eq!(error.context().current_attempt(), None);
        assert!(matches!(
            error.failure(),
            RetryFailure::Infrastructure {
                failure: RetryInfrastructureFailure::Clock { .. },
                last_failure: None,
                ..
            }
        ));
    }
}
