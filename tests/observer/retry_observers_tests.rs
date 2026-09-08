// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use std::future;
use std::future::Future;
use std::mem::forget;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::panic::panic_any;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::Timer;
use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;

use crate::support::TestError;

struct NoopObserver;

impl RetryObserver<()> for NoopObserver {}

#[test]
fn test_observers_are_registered_by_retry_builder() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).observer(NoopObserver).build();
    let _ = retry;
}

/// Completion observations captured before each callback advances the clock.
#[derive(Debug)]
struct CompletionRecord {
    index: usize,
    phase: RetryCallbackPhase,
    failure: Option<String>,
    context: RetryContext,
}

/// Independent terminal outcomes exercised through every applicable facade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionScenario {
    Success,
    Abort,
    Exhausted,
    ExhaustedBeforeAttempt,
    CancelledBeforeAttempt,
    CancelledAfterFailure,
    StartedPanic,
    FailedPanic,
    ScheduledPanic,
    RulePanic,
    TimerFailure,
    TimerFailureBeforeAttempt,
    TimedOut,
}

/// Observer that records completions and can panic in a control phase.
struct CompletionObserver {
    index: usize,
    panic_on_completion: bool,
    scenario: CompletionScenario,
    records: Arc<Mutex<Vec<CompletionRecord>>>,
    clock: Arc<ManualMonotonicClock>,
    started_calls: Arc<AtomicUsize>,
    cancellation: RetryCancellationToken,
}

impl CompletionObserver {
    /// Records a frozen result, advances virtual time, and optionally panics.
    fn complete(&self, phase: RetryCallbackPhase, failure: Option<String>, context: &RetryContext) {
        self.records
            .lock()
            .expect("completion records lock")
            .push(CompletionRecord {
                index: self.index,
                phase,
                failure,
                context: *context,
            });
        self.clock
            .advance(Duration::from_secs(10))
            .expect("advance completion clock");
        assert!(!self.panic_on_completion, "completion panic");
    }
}

impl RetryObserver<TestError> for CompletionObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        self.started_calls.fetch_add(1, Ordering::SeqCst);
        if self.index == 0 {
            assert_ne!(self.scenario, CompletionScenario::StartedPanic, "control panic");
        }
    }

    fn on_attempt_failed(&self, _failure: &AttemptFailure<TestError>, _context: &RetryContext) {
        if self.index == 0 {
            if self.scenario == CompletionScenario::CancelledAfterFailure {
                self.cancellation.cancel();
            }
            assert_ne!(self.scenario, CompletionScenario::FailedPanic, "control panic");
        }
    }

    fn on_retry_scheduled(&self, _backoff: &BackoffStep, _context: &RetryContext) {
        if self.index == 0 {
            assert_ne!(self.scenario, CompletionScenario::ScheduledPanic, "control panic");
            if self.scenario == CompletionScenario::TimedOut {
                self.clock
                    .advance(Duration::from_secs(1))
                    .expect("expire flow deadline");
            }
        }
    }

    fn on_success(&self, context: &RetryContext) {
        self.complete(RetryCallbackPhase::Success, None, context);
    }

    fn on_terminal_failure(&self, failure: &RetryErrorReason, context: &RetryContext) {
        self.complete(
            RetryCallbackPhase::TerminalFailure,
            Some(format!("{failure:?}")),
            context,
        );
    }
}

/// Facades sharing the completion contract; sync has no hard timeout.
#[derive(Clone, Copy, Debug)]
enum CompletionFacade {
    Sync,
    Worker,
    #[cfg(feature = "tokio")]
    Async,
}

/// Exercises real public execution and verifies frozen terminal diagnostics.
async fn assert_completion_case(facade: CompletionFacade, scenario: CompletionScenario, panic_on_completion: bool) {
    let clock = ManualMonotonicClock::new_shared();
    let records = Arc::new(Mutex::new(Vec::new()));
    let started_calls = Arc::new(AtomicUsize::new(0));
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let cancellation = RetryCancellationToken::new();
    if scenario == CompletionScenario::CancelledBeforeAttempt {
        cancellation.cancel();
    }
    let mut policy = RetryPolicy::builder().max_attempts(if scenario == CompletionScenario::Exhausted {
        1
    } else {
        2
    });
    if scenario == CompletionScenario::TimerFailure {
        policy = policy.backoff(BackoffPolicy::fixed(Duration::from_millis(1)));
    }
    if scenario == CompletionScenario::ExhaustedBeforeAttempt {
        policy = policy.total_time_budget(Duration::ZERO);
    }
    let mut builder =
        Retry::<TestError>::builder(policy.build().expect("valid completion policy")).fallback(RetryFallback::Retry);
    for index in 0..3 {
        builder = builder.observer(CompletionObserver {
            index,
            panic_on_completion: panic_on_completion && index < 2,
            scenario,
            records: Arc::clone(&records),
            clock: Arc::clone(&clock),
            started_calls: Arc::clone(&started_calls),
            cancellation: cancellation.clone(),
        });
    }
    let retry = builder
        .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            assert_ne!(scenario, CompletionScenario::RulePanic, "rule panic");
            if scenario == CompletionScenario::Abort {
                RetryDecision::Abort
            } else {
                RetryDecision::UseDefault
            }
        })
        .build();
    let timer: Arc<dyn Timer> = if matches!(
        scenario,
        CompletionScenario::TimerFailure | CompletionScenario::TimerFailureBeforeAttempt
    ) {
        Arc::new(FaultInjectingTimer::backend_unavailable(
            TimerFailurePoint::Registration,
            "completion",
            "offline",
        ))
    } else {
        clock.new_timer()
    };
    let operation = {
        let operation_calls = Arc::clone(&operation_calls);
        move || {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            if scenario == CompletionScenario::Success {
                Ok(42)
            } else {
                Err(TestError("original error"))
            }
        }
    };
    let result = match facade {
        CompletionFacade::Sync => retry
            .sync()
            .timer(timer)
            .cancellation_token(cancellation)
            .run(operation),
        CompletionFacade::Worker => {
            let mut worker = retry.worker().timer(timer).cancellation_token(cancellation);
            if matches!(
                scenario,
                CompletionScenario::TimedOut | CompletionScenario::TimerFailureBeforeAttempt
            ) {
                worker = worker.hard_flow_timeout(Duration::from_secs(1));
            }
            worker.run(move |_| operation())
        }
        #[cfg(feature = "tokio")]
        CompletionFacade::Async => {
            let mut executor = retry.tokio().timer(timer).cancellation_token(cancellation);
            if matches!(
                scenario,
                CompletionScenario::TimedOut | CompletionScenario::TimerFailureBeforeAttempt
            ) {
                executor = executor.hard_flow_timeout(Duration::from_secs(1));
            }
            executor.run(|| future::ready(operation())).await
        }
    };
    let (context, failures, phase, original_failure) = match result {
        Ok(success) => {
            assert_eq!(scenario, CompletionScenario::Success);
            assert_eq!(*success.value(), 42);
            assert_eq!(
                success.completion_callback_failures().len(),
                if panic_on_completion { 2 } else { 0 }
            );
            let (value, context, failures) = success.into_parts();
            assert_eq!(value, 42);
            (context, failures, RetryCallbackPhase::Success, None)
        }
        Err(error) => {
            assert_ne!(scenario, CompletionScenario::Success);
            let zero_attempts = matches!(
                scenario,
                CompletionScenario::CancelledBeforeAttempt
                    | CompletionScenario::ExhaustedBeforeAttempt
                    | CompletionScenario::StartedPanic
                    | CompletionScenario::TimerFailureBeforeAttempt
            );
            assert_eq!(error.context().attempts(), u32::from(!zero_attempts));
            assert_eq!(
                error.last_error(),
                (!zero_attempts).then_some(&TestError("original error")),
                "{facade:?} {scenario:?}"
            );
            match scenario {
                CompletionScenario::Abort => assert!(matches!(error.reason(), RetryErrorReason::Aborted)),
                CompletionScenario::Exhausted | CompletionScenario::ExhaustedBeforeAttempt => {
                    let expected = if zero_attempts {
                        RetryLimitKind::TotalElapsed
                    } else {
                        RetryLimitKind::Attempts
                    };
                    assert!(matches!(error.reason(), RetryErrorReason::Exhausted { limit, .. } if *limit == expected));
                }
                CompletionScenario::CancelledBeforeAttempt | CompletionScenario::CancelledAfterFailure => {
                    let expected = if zero_attempts {
                        RetryCancellationPhase::BeforeAttempt
                    } else {
                        RetryCancellationPhase::Backoff
                    };
                    assert!(matches!(error.reason(), RetryErrorReason::Cancelled { phase, .. } if *phase == expected));
                }
                CompletionScenario::StartedPanic
                | CompletionScenario::FailedPanic
                | CompletionScenario::ScheduledPanic
                | CompletionScenario::RulePanic => {
                    let expected = match scenario {
                        CompletionScenario::StartedPanic => RetryCallbackPhase::BeforeAttempt,
                        CompletionScenario::FailedPanic => RetryCallbackPhase::AttemptFailed,
                        CompletionScenario::ScheduledPanic => RetryCallbackPhase::RetryScheduled,
                        _ => RetryCallbackPhase::RuleDecision,
                    };
                    assert!(
                        matches!(error.reason(), RetryErrorReason::CallbackFailed { callback, .. } if callback.phase() == expected && callback.index() == 0)
                    );
                }
                CompletionScenario::TimerFailure | CompletionScenario::TimerFailureBeforeAttempt => {
                    assert!(matches!(
                        error.reason(),
                        RetryErrorReason::Infrastructure {
                            failure: RetryInfrastructureFailure::Timer { .. },
                            ..
                        }
                    ))
                }
                CompletionScenario::TimedOut => assert!(matches!(
                    error.reason(),
                    RetryErrorReason::TimedOut {
                        scope: RetryTimeoutScope::Flow,
                        ..
                    }
                )),
                CompletionScenario::Success => unreachable!(),
            }
            let original_failure = format!("{:?}", error.reason());
            assert_eq!(
                error.completion_callback_failures().len(),
                if panic_on_completion { 2 } else { 0 }
            );
            let (reason, _last_failure, context, failures) = error.into_parts();
            assert_eq!(format!("{reason:?}"), original_failure);
            (
                context,
                failures.into_vec(),
                RetryCallbackPhase::TerminalFailure,
                Some(original_failure),
            )
        }
    };
    assert_eq!(operation_calls.load(Ordering::SeqCst), context.attempts() as usize);
    if scenario == CompletionScenario::StartedPanic {
        assert_eq!(started_calls.load(Ordering::SeqCst), 1);
    }
    for (index, failure) in failures.iter().enumerate() {
        assert_eq!(failure.callback(), RetryCallbackKind::Observer);
        assert_eq!(failure.index(), index);
        assert_eq!(failure.phase(), phase);
        assert_eq!(failure.panic().message(), Some("completion panic"));
    }
    let records = records.lock().expect("completion records lock");
    assert_eq!(records.len(), 3, "each observer receives exactly one terminal callback");
    for (index, record) in records.iter().enumerate() {
        assert_eq!(record.index, index);
        assert_eq!(record.phase, phase);
        assert_eq!(record.failure, original_failure);
        assert_eq!(
            record.context, context,
            "completion callbacks must see the frozen context"
        );
    }
    if !matches!(
        scenario,
        CompletionScenario::TimerFailure | CompletionScenario::TimerFailureBeforeAttempt
    ) {
        let expected_elapsed = if scenario == CompletionScenario::TimedOut {
            Duration::from_secs(1)
        } else {
            Duration::ZERO
        };
        assert_eq!(
            context.total_elapsed(),
            expected_elapsed,
            "completion time must not enter the terminal context"
        );
    }
}

/// All ordinary Result returns share the completion contract in every facade.
#[tokio::test]
async fn test_completion_matrix_preserves_result_context_and_observer_order() {
    for facade in [
        CompletionFacade::Sync,
        CompletionFacade::Worker,
        #[cfg(feature = "tokio")]
        CompletionFacade::Async,
    ] {
        for scenario in [
            CompletionScenario::Success,
            CompletionScenario::Abort,
            CompletionScenario::Exhausted,
            CompletionScenario::ExhaustedBeforeAttempt,
            CompletionScenario::CancelledBeforeAttempt,
            CompletionScenario::CancelledAfterFailure,
            CompletionScenario::StartedPanic,
            CompletionScenario::FailedPanic,
            CompletionScenario::ScheduledPanic,
            CompletionScenario::RulePanic,
            CompletionScenario::TimerFailure,
            CompletionScenario::TimerFailureBeforeAttempt,
            CompletionScenario::TimedOut,
        ] {
            if matches!(facade, CompletionFacade::Sync)
                && matches!(
                    scenario,
                    CompletionScenario::TimerFailureBeforeAttempt | CompletionScenario::TimedOut
                )
            {
                continue;
            }
            for panic_on_completion in [false, true] {
                assert_completion_case(facade, scenario, panic_on_completion).await;
            }
        }
    }
}

/// Sync closures retain local borrows and may return a borrowed, non-Send
/// value.
#[test]
fn test_completion_sync_preserves_borrowed_non_send_operation() {
    let value = Rc::new(42);
    let mut calls = 0;
    let retry = Retry::<()>::builder(RetryPolicy::builder().build().expect("valid policy")).build();
    let success = retry
        .sync()
        .run(|| {
            calls += 1;
            Ok(&value)
        })
        .expect("borrowed success");
    assert_eq!(calls, 1);
    assert!(ptr::eq(*success.value(), &value));
    assert!(success.completion_callback_failures().is_empty());
}

/// Async operation futures remain non-Send and may borrow caller-owned data.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_completion_async_preserves_borrowed_non_send_future() {
    let value = Rc::new(42);
    let retry = Retry::<()>::builder(RetryPolicy::builder().build().expect("valid policy")).build();
    let success = retry
        .tokio()
        .run(|| async {
            let borrowed = &value;
            tokio::task::yield_now().await;
            Ok(borrowed)
        })
        .await
        .expect("non-Send borrowed success");
    assert!(ptr::eq(*success.value(), &value));
    assert!(success.completion_callback_failures().is_empty());
}

/// Counts completion callbacks without observing control callbacks.
struct CompletionCounter(Arc<AtomicUsize>);

impl RetryObserver<TestError> for CompletionCounter {
    fn on_success(&self, _context: &RetryContext) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn on_terminal_failure(&self, _failure: &RetryErrorReason, _context: &RetryContext) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Operation panics propagate from sync without synthesizing a Result.
#[test]
fn test_completion_sync_operation_panic_does_not_notify() {
    let calls = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder(RetryPolicy::builder().build().expect("valid policy"))
        .observer(CompletionCounter(Arc::clone(&calls)))
        .build();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = retry
            .sync()
            .run(|| -> Result<(), TestError> { panic!("operation panic") });
    }));
    assert!(panic.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// A started async operation remains unobserved when its future is dropped.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_completion_dropped_async_future_does_not_notify() {
    let calls = Arc::new(AtomicUsize::new(0));
    let operation_calls = AtomicUsize::new(0);
    let retry = Retry::<TestError>::builder(RetryPolicy::builder().build().expect("valid policy"))
        .observer(CompletionCounter(Arc::clone(&calls)))
        .build();
    let executor = retry.tokio();
    let mut future = Box::pin(executor.run(|| {
        operation_calls.fetch_add(1, Ordering::SeqCst);
        future::pending::<Result<(), TestError>>()
    }));
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
    drop(future);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// Async operation panics propagate without synthesizing a terminal Result.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_completion_async_operation_panic_does_not_notify() {
    let calls = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder(RetryPolicy::builder().build().expect("valid policy"))
        .observer(CompletionCounter(Arc::clone(&calls)))
        .build();
    let executor = retry.tokio();
    let mut future = Box::pin(executor.run(|| async {
        panic!("operation panic");
        #[allow(unreachable_code)]
        Ok::<(), TestError>(())
    }));
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = future.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    }));
    assert!(panic.is_err());
    drop(future);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// Panics at either completion boundary to exercise consuming result methods.
struct CompletionPanic;

impl RetryObserver<TestError> for CompletionPanic {
    fn on_success(&self, _context: &RetryContext) {
        panic!("completion panic");
    }

    fn on_terminal_failure(&self, _failure: &RetryErrorReason, _context: &RetryContext) {
        panic!("completion panic");
    }
}

/// Default decomposition preserves diagnostics; discarding uses explicit names.
#[test]
fn test_completion_result_consumers_preserve_or_explicitly_discard_diagnostics() {
    let retry = Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().expect("valid policy"))
        .observer(|_: &AttemptFailure<TestError>, _: &RetryContext| {})
        .observer(CompletionPanic)
        .fallback(RetryFallback::Retry)
        .build();
    let success = retry.sync().run(|| Ok(42)).expect("successful operation");
    assert_eq!(success.completion_callback_failures().len(), 1);
    assert_eq!(success.completion_callback_failures()[0].index(), 1);
    let cloned = success.clone();
    assert_eq!(success, cloned);
    assert_eq!(cloned.into_value_discarding_diagnostics(), 42);
    let (value, context, diagnostics) = success.into_parts();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].index(), 1);
    assert_eq!(value, 42);
    assert_eq!(context.attempts(), 1);

    let error = retry
        .sync()
        .run(|| Err::<(), _>(TestError("original error")))
        .expect_err("exhausted");
    assert_eq!(error.completion_callback_failures().len(), 1);
    assert_eq!(error.completion_callback_failures()[0].index(), 1);
    let (reason, last_failure, context, diagnostics) = error.into_parts();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].index(), 1);
    assert!(matches!(reason, RetryErrorReason::Exhausted { .. }));
    assert_eq!(
        last_failure.as_ref().and_then(AttemptFailure::as_error),
        Some(&TestError("original error"))
    );
    assert_eq!(context.attempts(), 1);
    let error = retry
        .sync()
        .run(|| Err::<(), _>(TestError("original error")))
        .expect_err("exhausted");
    assert_eq!(error.completion_callback_failures().len(), 1);
    assert_eq!(error.last_error(), Some(&TestError("original error")));
}

/// Non-string panic payload whose destructor raises another panic payload.
struct CompletionDropPanicPayload {
    drops: Arc<AtomicUsize>,
    recursive: bool,
}

impl Drop for CompletionDropPanicPayload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        if self.recursive {
            panic_any(Self {
                drops: Arc::clone(&self.drops),
                recursive: true,
            });
        }
        panic!("completion payload drop panic");
    }
}

/// Observer that panics with a payload carrying a panicking destructor.
struct CompletionDropPanicObserver {
    drops: Arc<AtomicUsize>,
    recursive: bool,
}

impl CompletionDropPanicObserver {
    /// Raises a real non-string panic with a destructor that also panics.
    fn raise(&self) {
        panic_any(CompletionDropPanicPayload {
            drops: Arc::clone(&self.drops),
            recursive: self.recursive,
        });
    }
}

impl RetryObserver<TestError> for CompletionDropPanicObserver {
    fn on_success(&self, _context: &RetryContext) {
        self.raise();
    }

    fn on_terminal_failure(&self, _failure: &RetryErrorReason, _context: &RetryContext) {
        self.raise();
    }
}

/// Payload destruction cannot discard either terminal outcome or stop later
/// observers, even when the secondary panic payload has another panicking Drop.
#[tokio::test]
async fn test_completion_payload_drop_panic_preserves_result_and_later_observers() {
    for facade in [
        CompletionFacade::Sync,
        CompletionFacade::Worker,
        #[cfg(feature = "tokio")]
        CompletionFacade::Async,
    ] {
        for successful in [true, false] {
            for recursive in [false, true] {
                let drops = Arc::new(AtomicUsize::new(0));
                let calls = Arc::new(AtomicUsize::new(0));
                let clock = ManualMonotonicClock::new_shared();
                let retry =
                    Retry::<TestError>::builder(RetryPolicy::builder().max_attempts(1).build().expect("valid policy"))
                        .fallback(RetryFallback::Retry)
                        .observer(CompletionDropPanicObserver {
                            drops: Arc::clone(&drops),
                            recursive,
                        })
                        .observer(CompletionCounter(Arc::clone(&calls)))
                        .build();
                let operation = move || {
                    if successful {
                        Ok(42)
                    } else {
                        Err(TestError("original error"))
                    }
                };
                let mut future = Box::pin(async {
                    match facade {
                        CompletionFacade::Sync => retry.sync().timer(clock.new_timer()).run(operation),
                        CompletionFacade::Worker => retry.worker().timer(clock.new_timer()).run(move |_| operation()),
                        #[cfg(feature = "tokio")]
                        CompletionFacade::Async => {
                            retry
                                .tokio()
                                .timer(clock.new_timer())
                                .run(|| future::ready(operation()))
                                .await
                        }
                    }
                });
                // Polling these immediate operations finishes in one poll. The
                // outer catch turns a leaked panic into a normal test failure;
                // forget its payload to avoid recursive Drop aborting the suite.
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    future.as_mut().poll(&mut Context::from_waker(Waker::noop()))
                }));
                let result = match outcome {
                    Ok(Poll::Ready(result)) => result,
                    Ok(Poll::Pending) => {
                        panic!("immediate completion must be ready")
                    }
                    Err(payload) => {
                        forget(payload);
                        panic!("completion panic payload destructor escaped");
                    }
                };
                let (context, failures, phase) = match result {
                    Ok(success) => {
                        assert!(successful);
                        let (value, context, failures) = success.into_parts();
                        assert_eq!(value, 42);
                        (context, failures, RetryCallbackPhase::Success)
                    }
                    Err(error) => {
                        assert!(!successful);
                        let (reason, last_failure, context, failures) = error.into_parts();
                        assert!(matches!(
                            reason,
                            RetryErrorReason::Exhausted {
                                limit: RetryLimitKind::Attempts,
                                ..
                            }
                        ));
                        assert_eq!(
                            last_failure.as_ref().and_then(AttemptFailure::as_error),
                            Some(&TestError("original error"))
                        );
                        (context, failures.into_vec(), RetryCallbackPhase::TerminalFailure)
                    }
                };
                assert_eq!(context.attempts(), 1);
                assert_eq!(context.total_elapsed(), Duration::ZERO);
                assert_eq!(failures.len(), 1);
                assert_eq!(failures[0].callback(), RetryCallbackKind::Observer);
                assert_eq!(failures[0].index(), 0);
                assert_eq!(failures[0].phase(), phase);
                assert_eq!(failures[0].panic(), &RetryPanic::NonString);
                assert_eq!(drops.load(Ordering::SeqCst), 1);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
            }
        }
    }
}
