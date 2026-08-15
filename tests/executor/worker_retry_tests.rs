// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AttemptCancellationToken;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

use crate::support::CountingPhaseObserver;
use crate::support::ElapsedObserverCallback;
use crate::support::ElapsedRuleCallback;
use crate::support::ObserverPhaseCounts;
use crate::support::PanickingPhaseObserver;
use crate::support::TestError;
use crate::support::assert_callback_panic_elapsed;
use crate::support::assert_matrix_abort;
use crate::support::assert_matrix_infrastructure;
use crate::support::assert_matrix_limit;
use crate::support::assert_matrix_observer_panic;
use crate::support::assert_matrix_rule_panic;
use crate::support::assert_matrix_timeout;
use crate::support::callback_elapsed_records;
use crate::support::completion_regressing_timer;
use crate::support::rule_terminal_regressing_timer;

#[test]
fn worker_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.worker();
}

#[test]
fn worker_spawn_failure_preserves_infrastructure_diagnostic() {
    let operation_calls = std::sync::Arc::new(AtomicUsize::new(0));
    let rule_calls = std::sync::Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy)
        .rule({
            let rule_calls = std::sync::Arc::clone(&rule_calls);
            move |_: &AttemptFailure<TestError>, _: &RetryContext| {
                rule_calls.fetch_add(1, Ordering::SeqCst);
                RetryDecision::Retry
            }
        })
        .build();

    let error = retry
        .worker()
        .worker_stack_size(usize::MAX)
        .run({
            let operation_calls = std::sync::Arc::clone(&operation_calls);
            move |_: AttemptCancellationToken| {
                operation_calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(TestError("operation should not run"))
            }
        })
        .unwrap_err();

    assert_eq!(operation_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
    let RetryFailure::Infrastructure {
        failure: RetryInfrastructureFailure::WorkerSpawn { message },
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a worker-spawn infrastructure failure");
    };
    assert!(!message.is_empty());
    assert!(last_failure.is_none());
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_attempt_timeout(), None);
}

#[test]
fn worker_retry_default_panic_survives_post_rule_clock_regression() {
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .build()
    .worker()
    .timer(rule_terminal_regressing_timer())
    .run(|_| -> Result<(), TestError> { panic!("operation panic") })
    .expect_err("the operation panic must remain the terminal cause");

    let RetryFailure::Aborted { last_failure, .. } = error.failure() else {
        panic!("expected abort instead of post-rule clock failure");
    };
    let AttemptFailure::Panicked { panic } = last_failure else {
        panic!("expected the operation panic as the last attempt failure");
    };
    assert_eq!(panic.message(), Some("operation panic"));
    assert_eq!(error.context().total_elapsed(), Duration::ZERO);
}

#[test]
fn worker_retry_matches_shared_terminal_matrix() {
    let abort = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Abort
    })
    .build()
    .worker()
    .run(|_| Err::<(), _>(TestError("matrix")))
    .expect_err("the explicit abort rule must terminate after attempt one");
    assert_matrix_abort(&abort);

    let attempts = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .worker()
    .run(|_| Err::<(), _>(TestError("matrix")))
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
        let operation_clock = Arc::clone(&clock);
        let error = Retry::<TestError>::builder(policy.build().unwrap())
            .build()
            .worker()
            .timer(clock.new_timer())
            .run(move |_| {
                operation_clock
                    .advance(Duration::from_secs(1))
                    .expect("manual matrix clock should advance");
                Err::<(), _>(TestError("matrix"))
            })
            .expect_err("the elapsed limit must reject continuation");
        assert_matrix_limit(&error, limit, 1, true);
    }
}

#[test]
fn worker_retry_matches_shared_callback_matrix() {
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
    .worker()
    .run(|_| Err::<(), _>(TestError("matrix")))
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
        .worker()
        .run(|_| Err::<(), _>(TestError("matrix")))
        .expect_err("the first panicking observer must fail closed");
        assert_matrix_observer_panic(&error, phase, later_counts.as_ref());
    }
}

#[test]
fn worker_retry_refreshes_elapsed_time_between_callback_phases() {
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
        .worker()
        .timer(clock.new_timer())
        .run(|_| Err::<(), _>(TestError("elapsed")))
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

#[test]
fn worker_retry_refreshes_elapsed_time_after_callback_panics() {
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
                .worker()
                .timer(clock.new_timer())
                .run(|_| Err::<(), _>(TestError("elapsed")))
        } else {
            Retry::<TestError>::builder(policy)
                .observer(ElapsedObserverCallback::new(
                    Arc::clone(&clock),
                    phase,
                    records,
                    true,
                ))
                .build()
                .worker()
                .timer(clock.new_timer())
                .run(|_| Err::<(), _>(TestError("elapsed")))
        }
        .expect_err("the advancing callback should panic");
        assert_callback_panic_elapsed(&error, phase);
    }
}

#[test]
fn worker_retry_matches_shared_infrastructure_and_timeout_matrix() {
    let timer_error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_millis(1)))
            .build()
            .unwrap(),
    )
    .build()
    .worker()
    .timer(Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "matrix",
        "offline",
    )))
    .run(|_| Err::<(), _>(TestError("matrix")))
    .expect_err("retry sleep registration failure must be terminal");
    assert_matrix_infrastructure(&timer_error, "timer", 1, None, true);

    let clock_error =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .worker()
            .timer(completion_regressing_timer())
            .run(|_| Ok::<_, TestError>(()))
            .expect_err("completion clock regression must be terminal");
    assert_matrix_infrastructure(&clock_error, "clock", 1, None, false);

    for scope in [RetryTimeoutScope::Attempt, RetryTimeoutScope::Flow] {
        let retry = Retry::<TestError>::builder(
            RetryPolicy::builder().build().unwrap(),
        )
        .build();
        let worker = retry.worker().cancellation_grace(Duration::from_secs(1));
        let worker = match scope {
            RetryTimeoutScope::Attempt => {
                worker.attempt_timeout(Duration::from_millis(1))
            }
            RetryTimeoutScope::Flow => {
                worker.flow_timeout(Duration::from_millis(1))
            }
        };
        let error = worker
            .run(|token| {
                while !token.is_cancelled() {
                    std::thread::yield_now();
                }
                Err::<(), _>(TestError("ignored after timeout"))
            })
            .expect_err("the cooperative worker must report its timeout");
        assert_matrix_timeout(&error, scope, 1);
    }
}

#[test]
fn worker_retry_reports_still_running_with_active_scope() {
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let error =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .worker()
            .attempt_timeout(Duration::from_millis(1))
            .cancellation_grace(Duration::from_millis(1))
            .run({
                let release_receiver = Arc::clone(&release_receiver);
                move |_| {
                    release_receiver
                        .lock()
                        .expect("release receiver lock should remain valid")
                        .recv()
                        .expect("test should release the detached worker");
                    Ok::<_, TestError>(())
                }
            })
            .expect_err(
                "a non-cooperative worker must remain structurally visible",
            );
    release_sender
        .send(())
        .expect("detached test worker should still receive its release");

    let RetryFailure::Infrastructure {
        failure: RetryInfrastructureFailure::WorkerStillRunning { trigger },
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a worker-still-running infrastructure failure");
    };
    assert_eq!(*trigger, WorkerStopTrigger::AttemptTimeout);
    assert_eq!(last_failure, &None);
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(
        error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
    assert_eq!(
        error.context().current_attempt_timeout(),
        Some(Duration::from_millis(1))
    );
}
