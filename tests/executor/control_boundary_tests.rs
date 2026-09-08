// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal accounting at each user-controlled callback boundary.

use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ClockDomain;
use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;
use qubit_retry::TokioRetry;
use qubit_retry::WorkerRetry;

/// Advances virtual time and cancels at precisely one selected control phase.
#[derive(Clone)]
struct CancellingControl {
    clock: Arc<ManualMonotonicClock>,
    token: RetryCancellationToken,
    phase: RetryCallbackPhase,
    fault: Option<(Arc<AtomicU8>, u8, bool)>,
}

impl CancellingControl {
    /// Simulates callback work without depending on OS scheduling.
    fn act(&self, phase: RetryCallbackPhase) {
        if self.phase == phase {
            self.clock.advance(Duration::from_secs(7)).expect("valid advance");
            self.token.cancel();
            if let Some((state, mode, panic_after)) = &self.fault {
                state.store(*mode, Ordering::SeqCst);
                assert!(!panic_after, "control callback panic after invalidating its clock");
            }
        }
    }
}

impl RetryObserver<&'static str> for CancellingControl {
    fn on_before_attempt(&self, _: &RetryContext) {
        self.act(RetryCallbackPhase::BeforeAttempt);
    }

    fn on_attempt_failed(&self, _: &AttemptFailure<&'static str>, _: &RetryContext) {
        self.act(RetryCallbackPhase::AttemptFailed);
    }

    fn on_retry_scheduled(&self, _: &BackoffStep, _: &RetryContext) {
        self.act(RetryCallbackPhase::RetryScheduled);
    }
}

/// Builds an execution whose selected observer or rule advances then cancels.
fn cancellation_case(
    phase: RetryCallbackPhase,
) -> (RetryConfig<&'static str>, Arc<ManualMonotonicClock>, RetryCancellationToken) {
    let clock = ManualMonotonicClock::new_shared();
    let token = RetryCancellationToken::new();
    let control = CancellingControl {
        clock: Arc::clone(&clock),
        token: token.clone(),
        phase,
        fault: None,
    };
    (build_case(control), clock, token)
}

/// Attaches the same boundary action through both observer and rule interfaces.
fn build_case(control: CancellingControl) -> RetryConfig<&'static str> {
    let rule = control.clone();
    RetryConfig::builder()
        .observer(control)
        .rule(move |_: &AttemptFailure<&'static str>, _: &RetryContext| {
            rule.act(RetryCallbackPhase::RuleDecision);
            RetryDecision::Retry
        })
        .build().expect("valid config")
}

/// Checks both elapsed accounting and terminal ownership of the attempt.
fn assert_cancelled(error: &RetryError<&'static str>, phase: RetryCallbackPhase) {
    let before = phase == RetryCallbackPhase::BeforeAttempt;
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(7), "{phase:?}");
    assert_eq!(error.context().operation_elapsed(), Duration::ZERO);
    assert_eq!(error.context().attempts(), u32::from(!before));
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().current_hard_attempt_timeout(), None);
    assert_eq!(error.last_error().copied(), (!before).then_some("offline"));
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled { phase, .. }
        if *phase == if before { RetryCancellationPhase::BeforeAttempt } else { RetryCancellationPhase::Backoff }));
    assert_eq!(
        error.context().next_delay(),
        (phase == RetryCallbackPhase::RetryScheduled).then_some(Duration::ZERO)
    );
}

/// The four callbacks have identical elapsed semantics across blocking modes.
#[test]
fn test_regression_control_cancellation_refreshes_blocking_context() {
    for phase in [
        RetryCallbackPhase::BeforeAttempt,
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RuleDecision,
        RetryCallbackPhase::RetryScheduled,
    ] {
        for worker in [false, true] {
            let (retry, clock, token) = cancellation_case(phase);
            let error = if worker {
                WorkerRetry::new(&retry)
                    .timer(clock.new_timer())
                    .cancellation_token(token)
                    .run(|_| Err::<(), _>("offline"))
            } else {
                Retry::new(&retry)
                    .timer(clock.new_timer())
                    .cancellation_token(token)
                    .run(|| Err::<(), _>("offline"))
            }
            .expect_err("callback cancellation terminates execution");
            assert_cancelled(&error, phase);
        }
    }
}

/// Tokio uses the same callback accounting contract without real-time waits.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_regression_control_cancellation_refreshes_async_context() {
    for phase in [
        RetryCallbackPhase::BeforeAttempt,
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RuleDecision,
        RetryCallbackPhase::RetryScheduled,
    ] {
        let (retry, clock, token) = cancellation_case(phase);
        let error = TokioRetry::new(&retry)
            .timer(clock.new_timer())
            .cancellation_token(token)
            .run(|| async { Err::<(), _>("offline") })
            .await
            .expect_err("callback cancellation terminates execution");
        assert_cancelled(&error, phase);
    }
}

/// Switches to a regressing or foreign-domain sample only when a callback acts.
#[derive(Clone)]
struct InvalidatingClock {
    base: Arc<ManualMonotonicClock>,
    foreign_domain: ClockDomain,
    mode: Arc<AtomicU8>,
}

impl MonotonicClock for InvalidatingClock {
    fn domain(&self) -> ClockDomain {
        self.base.domain()
    }

    fn now(&self) -> MonotonicInstant {
        match self.mode.load(Ordering::SeqCst) {
            1 => MonotonicInstant::new(self.domain(), Duration::ZERO),
            2 => MonotonicInstant::new(self.foreign_domain, Duration::from_secs(8)),
            _ => self.base.now(),
        }
    }

    fn new_timer(&self) -> Arc<dyn Timer> {
        Arc::new(InvalidatingTimer(self.clone()))
    }
}

/// Delegates timer waits to the manual source; only clock samples are faulted.
struct InvalidatingTimer(InvalidatingClock);

impl Timer for InvalidatingTimer {
    fn clock(&self) -> &dyn MonotonicClock {
        &self.0
    }

    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        self.0.base.new_timer().at(deadline)
    }
}

/// Creates a callback-triggered clock fault without brittle sample counting.
fn invalid_clock_case(
    phase: RetryCallbackPhase,
    mode: u8,
    panic_after: bool,
) -> (RetryConfig<&'static str>, Arc<dyn Timer>, RetryCancellationToken) {
    let base = ManualMonotonicClock::new_shared();
    base.advance(Duration::from_secs(1)).expect("nonzero initial sample");
    let clock = InvalidatingClock {
        base: Arc::clone(&base),
        foreign_domain: ManualMonotonicClock::new_shared().domain(),
        mode: Arc::new(AtomicU8::new(0)),
    };
    let token = RetryCancellationToken::new();
    let retry = build_case(CancellingControl {
        clock: base,
        token: token.clone(),
        phase,
        fault: Some((Arc::clone(&clock.mode), mode, panic_after)),
    });
    (retry, clock.new_timer(), token)
}

/// Invalid post-callback samples cannot replace a coherent snapshot.
fn assert_clock_terminal(error: &RetryError<&'static str>, phase: RetryCallbackPhase, panic_after: bool) {
    assert_eq!(error.context().total_elapsed(), Duration::ZERO);
    assert_eq!(
        error.context().attempts(),
        u32::from(phase != RetryCallbackPhase::BeforeAttempt)
    );
    assert_eq!(
        error.last_error().copied(),
        (phase != RetryCallbackPhase::BeforeAttempt).then_some("offline")
    );
    if panic_after {
        assert!(
            matches!(error.reason(), RetryErrorReason::CallbackFailed { callback, .. } if callback.phase() == phase)
        );
    } else {
        assert!(matches!(
            error.reason(),
            RetryErrorReason::Infrastructure {
                failure: RetryInfrastructureFailure::Clock { .. },
                ..
            }
        ));
        assert_eq!(error.context().current_attempt(), None);
    }
}

/// Normal callbacks require valid clocks; panicking callbacks retain
/// attribution.
#[test]
fn test_control_clock_failures_and_cancellation_have_explicit_precedence() {
    for phase in [
        RetryCallbackPhase::BeforeAttempt,
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RuleDecision,
        RetryCallbackPhase::RetryScheduled,
    ] {
        for mode in [1, 2] {
            for panic_after in [false, true] {
                for worker in [false, true] {
                    let (retry, timer, token) = invalid_clock_case(phase, mode, panic_after);
                    let error = if worker {
                        WorkerRetry::new(&retry)
                            .timer(timer)
                            .cancellation_token(token)
                            .run(|_| Err::<(), _>("offline"))
                    } else {
                        Retry::new(&retry)
                            .timer(timer)
                            .cancellation_token(token)
                            .run(|| Err::<(), _>("offline"))
                    }
                    .expect_err("invalid clock or callback panic must terminate");
                    assert_clock_terminal(&error, phase, panic_after);
                }
            }
        }
    }
}

/// Async execution has the same fault precedence at every control boundary.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_control_clock_failures_and_cancellation_have_explicit_precedence() {
    for phase in [
        RetryCallbackPhase::BeforeAttempt,
        RetryCallbackPhase::AttemptFailed,
        RetryCallbackPhase::RuleDecision,
        RetryCallbackPhase::RetryScheduled,
    ] {
        for mode in [1, 2] {
            for panic_after in [false, true] {
                let (retry, timer, token) = invalid_clock_case(phase, mode, panic_after);
                let error = TokioRetry::new(&retry)
                    .timer(timer)
                    .cancellation_token(token)
                    .run(|| async { Err::<(), _>("offline") })
                    .await
                    .expect_err("fault must terminate");
                assert_clock_terminal(&error, phase, panic_after);
            }
        }
    }
}
