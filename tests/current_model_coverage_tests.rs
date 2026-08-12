// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================

use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
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
use qubit_retry::AttemptFailureKind;
use qubit_retry::AttemptTimeoutKind;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryDiagnostic;
use qubit_retry::RetryDiagnosticKind;
use qubit_retry::RetryError;
use qubit_retry::RetryErrorKind;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryOutcomeKind;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryPolicyBuilder;
use qubit_retry::error::AttemptExecutionError;
use qubit_retry::error::RetryExecutionError;
use qubit_retry::error::RetryExecutionErrorKind;

use crate::support::FixedRetryRandomSource;
use crate::support::TestError;

#[test]
fn current_error_model_exposes_all_terminal_parts() {
    let execution = AttemptExecutionError::new("spawn failed");
    assert_eq!(execution.message(), "spawn failed");
    assert_eq!(execution.to_string(), "spawn failed");

    let failures = [
        AttemptFailure::Error(TestError("application")),
        AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Attempt,
        },
        AttemptFailure::Timeout {
            kind: AttemptTimeoutKind::Flow,
        },
        AttemptFailure::Panic,
        AttemptFailure::Infrastructure(execution.clone()),
    ];
    assert_eq!(failures[0].kind(), AttemptFailureKind::Application);
    assert_eq!(failures[1].kind(), AttemptFailureKind::TimedOut);
    assert_eq!(failures[3].kind(), AttemptFailureKind::Panicked);
    assert_eq!(failures[4].kind(), AttemptFailureKind::Infrastructure);
    assert_eq!(failures[0].as_error(), Some(&TestError("application")));
    assert!(failures[1].is_timeout());
    assert_eq!(
        failures[1].timeout_kind(),
        Some(AttemptTimeoutKind::Attempt)
    );
    assert_eq!(failures[2].timeout_kind(), Some(AttemptTimeoutKind::Flow));
    assert_eq!(failures[4].execution_error(), Some(&execution));
    assert!(failures[3].as_error().is_none());
    assert!(failures[0].timeout_kind().is_none());
    assert!(failures[0].execution_error().is_none());
    assert!(failures[3].clone().into_error().is_none());
    assert_eq!(
        failures[0].clone().into_error(),
        Some(TestError("application"))
    );
    assert_eq!(failures[0].to_string(), "application");
    assert!(failures[1].to_string().contains("Attempt"));
    assert_eq!(failures[3].to_string(), "attempt panicked");
    assert!(failures[4].to_string().contains("spawn failed"));

    let timer = RetryExecutionError::timer("timer unavailable");
    let worker = RetryExecutionError::worker("worker unavailable");
    let direct = RetryExecutionError::new(
        RetryExecutionErrorKind::Worker,
        "worker stopped",
    );
    assert_eq!(timer.kind(), RetryExecutionErrorKind::Timer);
    assert_eq!(timer.message(), "timer unavailable");
    assert_eq!(worker.kind(), RetryExecutionErrorKind::Worker);
    assert_eq!(direct.to_string(), "worker: worker stopped");
    assert_eq!(RetryExecutionErrorKind::Timer.to_string(), "timer");
    assert_eq!(RetryExecutionErrorKind::Worker.to_string(), "worker");

    let context = RetryContext::new(2, 3);
    let error = RetryError::<TestError>::new_with_execution_error(
        RetryErrorReason::TimerFailed,
        Some(AttemptFailure::<TestError>::Infrastructure(execution)),
        timer.clone(),
        context,
    );
    assert_eq!(error.reason(), RetryErrorReason::TimerFailed);
    assert_eq!(error.kind(), RetryErrorKind::Infrastructure);
    assert_eq!(error.attempts(), 2);
    assert_eq!(error.context(), &context);
    assert_eq!(error.execution_error(), Some(&timer));
    assert!(error.last_error().is_none());
    assert!(error.source().is_some());
    let (reason, failure, infrastructure, parts_context) =
        error.into_parts_with_execution_error();
    assert_eq!(reason, RetryErrorReason::TimerFailed);
    assert!(failure.is_some());
    assert!(infrastructure.is_some());
    assert_eq!(parts_context, context);

    let application = RetryError::new(
        RetryErrorReason::Aborted,
        Some(AttemptFailure::Error(TestError("fatal"))),
        context,
    );
    assert_eq!(application.last_error(), Some(&TestError("fatal")));
    assert_eq!(application.source().unwrap().to_string(), "fatal");
    assert!(application.to_string().contains("retry aborted"));
    let (reason, failure, _) = application.clone().into_parts();
    assert_eq!(reason, RetryErrorReason::Aborted);
    assert!(failure.is_some());
    assert_eq!(application.into_last_error(), Some(TestError("fatal")));

    for reason in [
        RetryErrorReason::AttemptsExhausted,
        RetryErrorReason::OperationBudgetExhausted,
        RetryErrorReason::TotalBudgetExhausted,
        RetryErrorReason::WorkerStillRunning,
        RetryErrorReason::AttemptTimedOut,
        RetryErrorReason::FlowTimedOut,
        RetryErrorReason::TimerFailed,
    ] {
        let rendered =
            RetryError::<TestError>::new(reason, None, context).to_string();
        assert!(!rendered.is_empty());
    }
    assert!(RetryErrorReason::OperationBudgetExhausted.is_elapsed_limit());
    assert!(RetryErrorReason::TotalBudgetExhausted.is_elapsed_limit());
    assert!(!RetryErrorReason::AttemptsExhausted.is_elapsed_limit());
    assert!(RetryErrorReason::TimerFailed.is_infrastructure_failure());
    assert!(RetryErrorReason::WorkerStillRunning.is_infrastructure_failure());
    assert!(!RetryErrorReason::Aborted.is_infrastructure_failure());
    assert_eq!(RetryErrorReason::Aborted.kind(), RetryErrorKind::Aborted);
    assert!(
        RetryError::<TestError>::new(
            RetryErrorReason::AttemptTimedOut,
            Some(AttemptFailure::Timeout {
                kind: AttemptTimeoutKind::Attempt,
            }),
            context,
        )
        .source()
        .is_none()
    );
    assert!(
        RetryError::<TestError>::new(
            RetryErrorReason::Aborted,
            Some(AttemptFailure::Panic),
            context,
        )
        .source()
        .is_none()
    );
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
    finished: AtomicU32,
    diagnostics: Mutex<Vec<(RetryDiagnosticKind, usize)>>,
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

    fn on_finished(&self, _outcome: RetryOutcomeKind, _context: &RetryContext) {
        self.0.finished.fetch_add(1, Ordering::SeqCst);
    }

    fn on_diagnostic(
        &self,
        diagnostic: &RetryDiagnostic,
        _context: &RetryContext,
    ) {
        self.0
            .diagnostics
            .lock()
            .unwrap()
            .push((diagnostic.kind(), diagnostic.callback_index()));
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

struct SecondRegistrationFailsTimer {
    clock: Arc<ManualMonotonicClock>,
    registrations: AtomicUsize,
}

impl SecondRegistrationFailsTimer {
    fn new() -> Self {
        Self {
            clock: ManualMonotonicClock::new_shared(),
            registrations: AtomicUsize::new(0),
        }
    }
}

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
    let attempts = AtomicU32::new(0);
    let result = Retry::<TestError>::builder(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::UseDefault
        })
        .observer(PanickingObserver)
        .observer(RecordingObserver(Arc::clone(&counts)))
        .build()
        .sync()
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(11_u32)
            }
        })
        .unwrap();
    let (_, context) = result.into_parts();
    assert_eq!(context.attempt(), 2);
    assert_eq!(counts.started.load(Ordering::SeqCst), 2);
    assert_eq!(counts.failed.load(Ordering::SeqCst), 1);
    assert_eq!(counts.scheduled.load(Ordering::SeqCst), 1);
    assert_eq!(counts.finished.load(Ordering::SeqCst), 1);
    let diagnostics = counts.diagnostics.lock().unwrap();
    assert!(diagnostics.contains(&(RetryDiagnosticKind::RulePanicked, 0)));
    assert!(diagnostics.contains(&(RetryDiagnosticKind::ObserverPanicked, 0)));
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
    assert_eq!(error.reason(), RetryErrorReason::TimerFailed);
    assert!(error.execution_error().is_some());

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
    assert_eq!(
        exhausted.reason(),
        RetryErrorReason::OperationBudgetExhausted
    );

    let aborted = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::Abort
        })
        .build()
        .sync()
        .run(|| Err::<(), _>(TestError("fatal")))
        .unwrap_err();
    assert_eq!(aborted.reason(), RetryErrorReason::Aborted);

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .sync()
    .run(|| Err::<(), _>(TestError("only attempt")))
    .unwrap_err();
    assert_eq!(
        attempts_exhausted.reason(),
        RetryErrorReason::AttemptsExhausted
    );

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
    assert_eq!(
        delay_rejected.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

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
    assert_eq!(
        expired_by_observer.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

    let attempts = AtomicU32::new(0);
    let explicit_delay = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryAfter(Duration::ZERO)
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
    assert_eq!(*explicit_delay.value(), 17);

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
    assert_eq!(timer_error.reason(), RetryErrorReason::TimerFailed);

    let panic_error = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .worker()
        .run(|_| -> Result<(), TestError> { panic!("isolated") })
        .unwrap_err();
    assert_eq!(panic_error.reason(), RetryErrorReason::Aborted);
    assert!(matches!(
        panic_error.last_failure(),
        Some(AttemptFailure::Panic)
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
    assert_eq!(detached.reason(), RetryErrorReason::WorkerStillRunning);
    assert_eq!(detached.context().unreaped_worker_count(), 1);

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .worker()
    .run(|_| Err::<(), _>(TestError("only attempt")))
    .unwrap_err();
    assert_eq!(
        attempts_exhausted.reason(),
        RetryErrorReason::AttemptsExhausted
    );

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
    assert_eq!(
        delay_rejected.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

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
    assert_eq!(
        expired_by_observer.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

    let rule_panics = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .build()
        .worker()
        .run(|_| Err::<(), _>(TestError("retry")))
        .unwrap_err();
    assert_eq!(rule_panics.reason(), RetryErrorReason::AttemptsExhausted);

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
    assert_eq!(zero_budget.reason(), RetryErrorReason::TotalBudgetExhausted);

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
    assert_eq!(cap_error.reason(), RetryErrorReason::TimerFailed);

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
    assert_eq!(explicit_retry.reason(), RetryErrorReason::AttemptsExhausted);
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
    assert_eq!(error.reason(), RetryErrorReason::TimerFailed);
    assert!(error.execution_error().is_some());

    let attempts_exhausted = Retry::<TestError>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .asynchronous()
    .run(|| async { Err::<(), _>(TestError("only attempt")) })
    .await
    .unwrap_err();
    assert_eq!(
        attempts_exhausted.reason(),
        RetryErrorReason::AttemptsExhausted
    );

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
    assert_eq!(
        delay_rejected.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

    let aborted = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::Abort
        })
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("fatal")) })
        .await
        .unwrap_err();
    assert_eq!(aborted.reason(), RetryErrorReason::Aborted);

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
    assert_eq!(
        expired_by_observer.reason(),
        RetryErrorReason::TotalBudgetExhausted
    );

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
    assert_eq!(
        attempt_registration_error.reason(),
        RetryErrorReason::TimerFailed
    );

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
    assert_eq!(
        attempt_completion_error.reason(),
        RetryErrorReason::TimerFailed
    );

    let rule_panics = Retry::<TestError>::builder(retry_once_policy())
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            panic!("rule panic")
        })
        .build()
        .asynchronous()
        .run(|| async { Err::<(), _>(TestError("retry")) })
        .await
        .unwrap_err();
    assert_eq!(rule_panics.reason(), RetryErrorReason::AttemptsExhausted);

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
    assert_eq!(zero_budget.reason(), RetryErrorReason::TotalBudgetExhausted);

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
    assert_eq!(
        tie.last_failure().and_then(AttemptFailure::timeout_kind),
        Some(AttemptTimeoutKind::Attempt)
    );

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
    assert_eq!(cap_error.reason(), RetryErrorReason::TimerFailed);

    let zero_flow = Retry::<TestError>::builder(retry_once_policy())
        .build()
        .asynchronous()
        .flow_timeout(Duration::ZERO)
        .run(|| async { Ok::<_, TestError>(()) })
        .await
        .unwrap_err();
    assert_eq!(zero_flow.reason(), RetryErrorReason::FlowTimedOut);

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
    assert_eq!(
        flow_expired_by_observer.reason(),
        RetryErrorReason::FlowTimedOut
    );

    let mut thread_random = BackoffPolicy::uniform(
        Duration::from_nanos(1),
        Duration::from_nanos(2),
    )
    .unwrap()
    .with_full_jitter()
    .start();
    let _ = thread_random.next(BackoffRequest::policy());
}
