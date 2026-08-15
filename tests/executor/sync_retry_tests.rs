// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
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
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

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
use crate::support::callback_elapsed_records;
use crate::support::completion_regressing_timer;
use crate::support::rule_terminal_regressing_timer;

#[test]
fn sync_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.sync();
}

/// Clock that moves backward only when a successful operation is finalized.
struct SuccessCompletionRegressingClock {
    domain: ClockDomain,
    samples: AtomicUsize,
}

impl SuccessCompletionRegressingClock {
    /// Creates a scripted clock for the five sync-facade samples.
    fn new() -> Self {
        Self {
            domain: ClockDomain::new(),
            samples: AtomicUsize::new(0),
        }
    }
}

impl MonotonicClock for SuccessCompletionRegressingClock {
    /// Returns the stable test domain.
    fn domain(&self) -> ClockDomain {
        self.domain
    }

    /// Returns an earlier instant on the success-completion sample.
    fn now(&self) -> MonotonicInstant {
        let sample = self.samples.fetch_add(1, Ordering::SeqCst);
        let elapsed = if sample < 4 {
            Duration::from_secs(1)
        } else {
            Duration::ZERO
        };
        MonotonicInstant::new(self.domain, elapsed)
    }

    /// This test clock is only exposed through its companion timer.
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("the scripted test clock does not create nested timers")
    }
}

/// Timer exposing [`SuccessCompletionRegressingClock`] to the sync facade.
struct SuccessCompletionRegressingTimer {
    clock: SuccessCompletionRegressingClock,
}

impl SuccessCompletionRegressingTimer {
    /// Creates the scripted timer.
    fn new() -> Self {
        Self {
            clock: SuccessCompletionRegressingClock::new(),
        }
    }
}

impl Timer for SuccessCompletionRegressingTimer {
    /// Returns the timer's scripted clock.
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }

    /// Returns an immediately ready notification for unused sleep requests.
    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        deadline.validate_domain(self.clock.domain())?;
        Ok(Box::pin(async { Ok(()) }))
    }
}

#[test]
fn sync_retry_success_clock_regression_returns_infrastructure_error() {
    let policy = RetryPolicy::builder().build().unwrap();
    let error = Retry::<TestError>::builder(policy)
        .build()
        .sync()
        .timer(Arc::new(SuccessCompletionRegressingTimer::new()))
        .run(|| Ok::<_, TestError>(42_u32))
        .expect_err("a regressing completion sample must fail");

    assert!(matches!(
        error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Clock { .. },
            last_failure: None,
            ..
        }
    ));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_attempt_timeout(), None);
}

#[test]
fn sync_retry_abort_survives_post_rule_clock_regression() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let error = Retry::<TestError>::builder(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::Abort
        })
        .build()
        .sync()
        .timer(rule_terminal_regressing_timer())
        .run(|| Err::<(), _>(TestError("abort")))
        .expect_err("the abort decision must remain the terminal cause");

    let RetryFailure::Aborted { last_failure, .. } = error.failure() else {
        panic!("expected abort instead of post-rule clock failure");
    };
    assert_eq!(last_failure, &AttemptFailure::Error(TestError("abort")));
    assert_eq!(error.context().total_elapsed(), Duration::ZERO);
}

/// Verifies the current-attempt overlay at each callback phase.
struct AttemptScopeObserver;

impl RetryObserver<TestError> for AttemptScopeObserver {
    /// Checks the pre-operation callback snapshot.
    fn on_attempt_started(&self, context: &RetryContext) {
        assert_eq!(
            context.current_attempt().map(|value| value.get()),
            Some(context.attempts() + 1)
        );
    }

    /// Checks the failed-operation callback snapshot.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        context: &RetryContext,
    ) {
        assert_eq!(context.attempts(), 1);
        assert_eq!(context.current_attempt().map(|value| value.get()), Some(1));
    }

    /// Checks the retry-scheduled callback snapshot.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        context: &RetryContext,
    ) {
        assert_eq!(context.attempts(), 1);
        assert_eq!(context.current_attempt().map(|value| value.get()), Some(1));
    }
}

#[test]
fn sync_retry_callbacks_retain_current_attempt_scope() {
    let attempts = AtomicUsize::new(0);
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let result = Retry::<TestError>::builder(policy)
        .observer(AttemptScopeObserver)
        .rule(|_: &AttemptFailure<TestError>, context: &RetryContext| {
            assert_eq!(context.attempts(), 1);
            assert_eq!(
                context.current_attempt().map(|value| value.get()),
                Some(1)
            );
            RetryDecision::Retry
        })
        .build()
        .sync()
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(())
            }
        })
        .expect("the second operation should succeed");

    assert_eq!(result.context().attempts(), 2);
    assert_eq!(result.context().current_attempt(), None);
}

struct ExhaustsBeforeSecondAttempt(Arc<ManualMonotonicClock>);

impl RetryObserver<TestError> for ExhaustsBeforeSecondAttempt {
    fn on_attempt_started(&self, context: &RetryContext) {
        let current_attempt = context
            .current_attempt()
            .expect("a started attempt must have a current attempt")
            .get();
        if current_attempt == 2 {
            self.0
                .advance(Duration::from_secs(1))
                .expect("manual clock should advance");
        }
    }
}

#[test]
fn sync_retry_preserves_last_failure_when_next_attempt_is_rejected() {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .max_total_elapsed(Duration::from_secs(1))
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let attempts = AtomicUsize::new(0);
    let error = Retry::<TestError>::builder(policy)
        .observer(ExhaustsBeforeSecondAttempt(Arc::clone(&clock)))
        .build()
        .sync()
        .timer(Arc::new(clock.new_timer()))
        .run(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(TestError("first attempt failed"))
        })
        .unwrap_err();

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(matches!(
        error.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            last_failure: Some(AttemptFailure::Error(TestError(
                "first attempt failed"
            ))),
            ..
        }
    ));
}

#[test]
fn sync_retry_matches_shared_terminal_matrix() {
    let abort = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(2).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Abort
    })
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("matrix")))
    .expect_err("the explicit abort rule must terminate after attempt one");
    assert_matrix_abort(&abort);

    let attempts = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("matrix")))
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
            .sync()
            .timer(clock.new_timer())
            .run(|| {
                clock
                    .advance(Duration::from_secs(1))
                    .expect("manual matrix clock should advance");
                Err::<(), _>(TestError("matrix"))
            })
            .expect_err("the elapsed limit must reject continuation");
        assert_matrix_limit(&error, limit, 1, true);
    }
}

#[test]
fn sync_retry_matches_shared_callback_matrix() {
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
    .sync()
    .run(|| Err::<(), _>(TestError("matrix")))
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
        .sync()
        .run(|| Err::<(), _>(TestError("matrix")))
        .expect_err("the first panicking observer must fail closed");
        assert_matrix_observer_panic(&error, phase, later_counts.as_ref());
    }
}

#[test]
fn sync_retry_refreshes_elapsed_time_between_callback_phases() {
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
        .sync()
        .timer(clock.new_timer())
        .run(|| Err::<(), _>(TestError("elapsed")))
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
fn sync_retry_refreshes_elapsed_time_after_callback_panics() {
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
                .sync()
                .timer(clock.new_timer())
                .run(|| Err::<(), _>(TestError("elapsed")))
        } else {
            Retry::<TestError>::builder(policy)
                .observer(ElapsedObserverCallback::new(
                    Arc::clone(&clock),
                    phase,
                    records,
                    true,
                ))
                .build()
                .sync()
                .timer(clock.new_timer())
                .run(|| Err::<(), _>(TestError("elapsed")))
        }
        .expect_err("the advancing callback should panic");
        assert_callback_panic_elapsed(&error, phase);
    }
}

#[test]
fn sync_retry_matches_shared_infrastructure_matrix() {
    // Attempt and flow timeout cases are intentionally absent: SyncRetry does
    // not expose a timeout API and cannot preempt a same-thread operation.
    let timer_error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::fixed(Duration::from_millis(1)))
            .build()
            .unwrap(),
    )
    .build()
    .sync()
    .timer(Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "matrix",
        "offline",
    )))
    .run(|| Err::<(), _>(TestError("matrix")))
    .expect_err("retry sleep registration failure must be terminal");
    assert_matrix_infrastructure(&timer_error, "timer", 1, None, true);

    let clock_error =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .sync()
            .timer(completion_regressing_timer())
            .run(|| Ok::<_, TestError>(()))
            .expect_err("completion clock regression must be terminal");
    assert_matrix_infrastructure(&clock_error, "clock", 1, None, false);
}
