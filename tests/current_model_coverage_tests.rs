// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
#[cfg(feature = "tokio")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
#[cfg(feature = "tokio")]
use qubit_clock::MonotonicInstant;
#[cfg(feature = "tokio")]
use qubit_clock::TimeError;
use qubit_clock::Timer;
#[cfg(feature = "tokio")]
use qubit_clock::TimerFuture;
use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryPolicyBuilder;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

use crate::support::FixedRetryRandomSource;
use crate::support::TestError;

#[test]
fn current_error_model_exposes_all_terminal_parts() {
    let failures = [
        AttemptFailure::Error(TestError("application")),
        AttemptFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        },
        AttemptFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
        },
        AttemptFailure::Panicked {
            panic: RetryPanic::StaticStr("isolated"),
        },
    ];
    assert_eq!(failures[0].as_error(), Some(&TestError("application")));
    assert!(failures[1].is_timeout());
    assert_eq!(
        failures[1].timeout_scope(),
        Some(RetryTimeoutScope::Attempt)
    );
    assert_eq!(failures[2].timeout_scope(), Some(RetryTimeoutScope::Flow));
    assert_eq!(
        failures[3].panic(),
        Some(&RetryPanic::StaticStr("isolated"))
    );
    assert!(failures[3].as_error().is_none());
    assert!(failures[0].timeout_scope().is_none());
    assert!(failures[0].panic().is_none());
    assert!(failures[3].clone().into_error().is_none());
    assert_eq!(
        failures[0].clone().into_error(),
        Some(TestError("application"))
    );
    assert_eq!(failures[0].to_string(), "application");
    assert_eq!(failures[1].to_string(), "attempt timed out (attempt)");
    assert_eq!(failures[3].to_string(), "attempt panicked: isolated");
}

#[test]
fn current_policy_builders_cover_backoff_variants() {
    let deterministic = Arc::new(FixedRetryRandomSource::new(0.5));
    let mut immediate = BackoffPolicy::immediate().start();
    assert_eq!(
        BackoffPolicy::immediate().maximum_delay(),
        Some(Duration::ZERO)
    );
    assert_eq!(
        immediate.next(BackoffRequest::policy()).effective_delay(),
        Duration::ZERO
    );

    let fixed = BackoffPolicy::fixed(Duration::from_millis(20));
    assert_eq!(fixed.maximum_delay(), Some(Duration::from_millis(20)));
    let mut fixed_state = fixed
        .clone()
        .without_jitter()
        .ignore_retry_after()
        .start_with_random_source(deterministic.clone());
    assert_eq!(
        fixed_state
            .next(BackoffRequest::hint(Duration::from_millis(50)))
            .effective_delay(),
        Duration::from_millis(20)
    );

    let uniform = BackoffPolicy::uniform(
        Duration::from_millis(10),
        Duration::from_millis(30),
    )
    .unwrap()
    .with_full_jitter()
    .prefer_retry_after();
    assert_eq!(uniform.maximum_delay(), Some(Duration::from_millis(30)));
    let mut uniform_state =
        uniform.start_with_random_source(deterministic.clone());
    let hinted = uniform_state
        .next(BackoffRequest::jittered_hint(Duration::from_millis(15)));
    assert_eq!(hinted.retry_index(), 1);
    assert!(hinted.base_delay() <= Duration::from_millis(30));
    assert!(hinted.effective_delay() <= Duration::from_millis(15));
    let _ = hinted.source();

    let exponential = BackoffPolicy::exponential(
        Duration::from_millis(5),
        2.0,
        Duration::from_millis(40),
    )
    .unwrap()
    .with_bounded_jitter(0.25)
    .unwrap()
    .use_retry_after_as_minimum();
    assert_eq!(exponential.maximum_delay(), Some(Duration::from_millis(40)));
    let mut exponential_state =
        exponential.start_with_random_source(deterministic);
    assert!(
        exponential_state
            .next(BackoffRequest::hint(Duration::from_millis(12)))
            .effective_delay()
            >= Duration::from_millis(12)
    );

    for invalid in [f64::NAN, -0.1, 1.1] {
        let error = BackoffPolicy::immediate()
            .with_bounded_jitter(invalid)
            .expect_err("invalid jitter must fail");
        assert_eq!(error.field(), "backoff.jitter.ratio");
        assert!(!error.message().is_empty());
        assert!(error.to_string().contains("backoff.jitter.ratio"));
    }

    let policy = RetryPolicyBuilder::new()
        .max_attempts(4)
        .max_operation_elapsed(Duration::from_secs(1))
        .max_operation_elapsed_opt(Some(Duration::from_secs(2)))
        .without_operation_elapsed()
        .max_total_elapsed(Duration::from_secs(3))
        .max_total_elapsed_opt(Some(Duration::from_secs(4)))
        .without_total_elapsed()
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    assert_eq!(policy.limits().max_attempts().get(), 4);
    assert_eq!(
        RetryPolicyBuilder::default()
            .build()
            .unwrap()
            .limits()
            .max_attempts()
            .get(),
        3
    );

    let mut saturated =
        BackoffPolicy::exponential(Duration::MAX, f64::MAX, Duration::MAX)
            .unwrap()
            .with_bounded_jitter(1.0)
            .unwrap()
            .start_with_random_source(Arc::new(FixedRetryRandomSource::new(
                1.0,
            )));
    let _ = saturated.next(BackoffRequest::policy());
    let _ = saturated.next(BackoffRequest::policy());
    let mut equal_uniform = BackoffPolicy::uniform(
        Duration::from_millis(4),
        Duration::from_millis(4),
    )
    .unwrap()
    .with_full_jitter()
    .start_with_random_source(Arc::new(FixedRetryRandomSource::new(0.5)));
    let _ = equal_uniform.next(BackoffRequest::policy());
}

#[derive(Default)]
struct LifecycleCounts {
    started: AtomicU32,
    failed: AtomicU32,
    scheduled: AtomicU32,
}

struct RecordingObserver(Arc<LifecycleCounts>);

impl RetryObserver<TestError> for RecordingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        self.0.started.fetch_add(1, Ordering::SeqCst);
    }

    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        self.0.failed.fetch_add(1, Ordering::SeqCst);
    }

    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.0.scheduled.fetch_add(1, Ordering::SeqCst);
    }
}

struct PanickingObserver;

impl RetryObserver<TestError> for PanickingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        panic!("observer panic");
    }
}

struct AdvancingObserver(Arc<ManualMonotonicClock>);

impl RetryObserver<TestError> for AdvancingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        self.0.advance(Duration::from_secs(2)).unwrap();
    }
}

struct DefaultObserver;

impl RetryObserver<TestError> for DefaultObserver {}

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

    fn at(
        &self,
        _deadline: MonotonicInstant,
    ) -> Result<TimerFuture, TimeError> {
        if self.registrations.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(Box::pin(std::future::pending()))
        } else {
            Err(TimeError::InstantOverflow)
        }
    }
}

#[test]
fn observers_and_rules_cover_current_lifecycle() {
    let counts = Arc::new(LifecycleCounts::default());
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let observer_error = Retry::<TestError>::builder(policy.clone())
        .observer(PanickingObserver)
        .observer(RecordingObserver(Arc::clone(&counts)))
        .build()
        .sync()
        .run(|| Ok::<_, TestError>(11_u32))
        .expect_err("the first started observer panic must terminate the flow");
    let RetryFailure::CallbackFailed {
        callback,
        last_failure,
        ..
    } = observer_error.failure()
    else {
        panic!("expected an observer callback failure");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), RetryCallbackPhase::AttemptStarted);
    assert_eq!(last_failure, &None);
    assert_eq!(observer_error.context().attempts(), 0);
    assert_eq!(
        observer_error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
    assert_eq!(counts.started.load(Ordering::SeqCst), 0);

    let rule_error = Retry::<TestError>::builder(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::UseDefault
        })
        .observer(RecordingObserver(Arc::clone(&counts)))
        .build()
        .sync()
        .run(|| Err::<u32, _>(TestError("retry")))
        .expect_err("the first rule panic must terminate the flow");
    let RetryFailure::CallbackFailed {
        callback,
        last_failure,
        ..
    } = rule_error.failure()
    else {
        panic!("expected a rule callback failure");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Rule);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), RetryCallbackPhase::RuleDecision);
    assert_eq!(
        last_failure,
        &Some(AttemptFailure::Error(TestError("retry")))
    );
    assert_eq!(counts.failed.load(Ordering::SeqCst), 1);
    assert_eq!(counts.scheduled.load(Ordering::SeqCst), 0);
    assert_eq!(
        rule_error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
}

fn retry_once_policy() -> RetryPolicy {
    RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::from_millis(1)))
        .build()
        .unwrap()
}

#[test]
fn sync_facade_reports_timer_and_budget_boundaries() {
    let timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
            TimerFailurePoint::Registration,
            "retry-test",
            "offline",
        ));
    let random = Arc::new(FixedRetryRandomSource::new(0.5));
    let error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .sync()
        .timer(timer)
        .random_source(random)
        .run(|| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        error.failure(),
        RetryFailure::Infrastructure {
            failure: RetryInfrastructureFailure::Timer { .. },
            ..
        }
    ));
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_attempt_timeout(), None);

    let exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_operation_elapsed(Duration::ZERO)
            .build()
            .unwrap(),
    )
    .build()
    .sync()
    .run(|| Ok::<_, TestError>(()))
    .unwrap_err();
    assert!(matches!(
        exhausted.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::OperationElapsed,
            ..
        }
    ));

    let aborted = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::Abort
        })
        .build()
        .sync()
        .run(|| Err::<(), _>(TestError("fatal")))
        .unwrap_err();
    assert!(matches!(aborted.failure(), RetryFailure::Aborted { .. }));

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("only attempt")))
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
    .sync()
    .run(|| Err::<(), _>(TestError("retry")))
    .unwrap_err();
    assert!(matches!(
        delay_rejected.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let clock = ManualMonotonicClock::new_shared();
    let observer = AdvancingObserver(Arc::clone(&clock));
    let expired_by_observer = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_total_elapsed(Duration::from_secs(1))
            .build()
            .unwrap(),
    )
    .observer(observer)
    .build()
    .sync()
    .timer(clock.new_timer())
    .run(|| Ok::<_, TestError>(()))
    .unwrap_err();
    assert!(matches!(
        expired_by_observer.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::TotalElapsed,
            ..
        }
    ));

    let attempts = AtomicU32::new(0);
    let hinted_retry = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithHint(Duration::ZERO)
        })
        .observer(DefaultObserver)
        .build()
        .sync()
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
    let jittered_retry = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithJitteredHint(Duration::ZERO)
        })
        .build()
        .sync()
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
    let _ = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .observer(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
        callback_count_for_observer.fetch_add(1, Ordering::SeqCst);
    })
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("observed")));
    assert_eq!(callback_count.load(Ordering::SeqCst), 1);
}

#[test]
fn worker_facade_reports_timer_panic_and_detached_worker() {
    let timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
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

    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let detached = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::from_millis(1))
        .run({
            let release_receiver = Arc::clone(&release_receiver);
            move |_| {
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
    assert_eq!(
        detached
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
    assert_eq!(
        detached.context().current_attempt_timeout(),
        Some(Duration::from_millis(1))
    );

    let zero_grace = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::ZERO)
        .run(|token| {
            while !token.is_cancelled() {
                std::thread::yield_now();
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

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
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
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .build()
        .worker()
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert!(matches!(
        rule_panics.failure(),
        RetryFailure::CallbackFailed { .. }
    ));

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

    let cap_timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
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

    let explicit_retry = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::Retry
    })
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
fn worker_facade_accepts_thread_name() {
    let result = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .worker()
    .thread_name("coverage-worker")
    .run(|_| Ok::<_, TestError>(()))
    .expect("named worker should start");

    assert_eq!(*result.value(), ());
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_facade_reports_timer_failure_with_injected_components() {
    let timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
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

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
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
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::Abort
        })
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

    let registration_timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
            TimerFailurePoint::Registration,
            "retry-test",
            "offline",
        ));
    let attempt_registration_error =
        Retry::<TestError>::builder(retry_once_policy())
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

    let completion_timer: Arc<dyn Timer> =
        Arc::new(FaultInjectingTimer::backend_unavailable(
            TimerFailurePoint::Completion,
            "retry-test",
            "offline",
        ));
    let attempt_completion_error =
        Retry::<TestError>::builder(retry_once_policy())
            .build()
            .asynchronous()
            .attempt_timeout(Duration::from_secs(1))
            .timer(completion_timer)
            .run(std::future::pending::<Result<(), TestError>>)
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
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert!(matches!(
        rule_panics.failure(),
        RetryFailure::CallbackFailed { .. }
    ));

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

    let successful_timed_attempt =
        Retry::<TestError>::builder(retry_once_policy())
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
        .run(std::future::pending::<Result<(), TestError>>)
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

    let cap_timer: Arc<dyn Timer> =
        Arc::new(SecondRegistrationFailsTimer::new());
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
    let flow_expired_by_observer =
        Retry::<TestError>::builder(retry_once_policy())
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
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithJitteredHint(Duration::ZERO)
        })
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

    let exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("only attempt")) })
    .await
    .unwrap_err();
    assert!(matches!(
        exhausted.failure(),
        RetryFailure::Exhausted {
            limit: RetryLimitKind::Attempts,
            ..
        }
    ));

    let mut thread_random = BackoffPolicy::uniform(
        Duration::from_nanos(1),
        Duration::from_nanos(2),
    )
    .unwrap()
    .with_full_jitter()
    .start();
    let _ = thread_random.next(BackoffRequest::policy());
}
