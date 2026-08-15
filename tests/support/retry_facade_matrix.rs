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

use qubit_clock::ClockDomain;
use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffStep;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryRule;
use qubit_retry::RetryTimeoutScope;

use super::TestError;

/// Ordered callback phase and elapsed-time observations.
pub(crate) type CallbackElapsedRecords =
    Arc<Mutex<Vec<(RetryCallbackPhase, Duration)>>>;

/// Observer that records and advances one selected callback phase.
pub(crate) struct ElapsedObserverCallback {
    clock: Arc<ManualMonotonicClock>,
    phase: RetryCallbackPhase,
    records: CallbackElapsedRecords,
    panic_after_advance: bool,
}

impl ElapsedObserverCallback {
    /// Creates an observer for one phase of the elapsed-time matrix.
    pub(crate) fn new(
        clock: Arc<ManualMonotonicClock>,
        phase: RetryCallbackPhase,
        records: CallbackElapsedRecords,
        panic_after_advance: bool,
    ) -> Self {
        Self {
            clock,
            phase,
            records,
            panic_after_advance,
        }
    }

    /// Records the supplied context, advances one second, and optionally
    /// panics.
    fn observe(&self, phase: RetryCallbackPhase, context: &RetryContext) {
        if self.phase != phase {
            return;
        }
        self.records
            .lock()
            .expect("callback elapsed records should not be poisoned")
            .push((phase, context.total_elapsed()));
        self.clock
            .advance(Duration::from_secs(1))
            .expect("callback elapsed clock should advance");
        assert!(!self.panic_after_advance, "elapsed observer panic");
    }
}

impl RetryObserver<TestError> for ElapsedObserverCallback {
    /// Records and advances the attempt-failed phase when selected.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        context: &RetryContext,
    ) {
        self.observe(RetryCallbackPhase::AttemptFailed, context);
    }

    /// Records and advances the retry-scheduled phase when selected.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        context: &RetryContext,
    ) {
        self.observe(RetryCallbackPhase::RetryScheduled, context);
    }
}

/// Rule that records its fresh context and advances the manual clock.
pub(crate) struct ElapsedRuleCallback {
    clock: Arc<ManualMonotonicClock>,
    records: CallbackElapsedRecords,
    panic_after_advance: bool,
}

impl ElapsedRuleCallback {
    /// Creates a rule for the elapsed-time matrix.
    pub(crate) fn new(
        clock: Arc<ManualMonotonicClock>,
        records: CallbackElapsedRecords,
        panic_after_advance: bool,
    ) -> Self {
        Self {
            clock,
            records,
            panic_after_advance,
        }
    }
}

impl RetryRule<TestError> for ElapsedRuleCallback {
    /// Records the rule context, advances one second, and optionally panics.
    fn decide(
        &self,
        _failure: &AttemptFailure<TestError>,
        context: &RetryContext,
    ) -> RetryDecision {
        self.records
            .lock()
            .expect("callback elapsed records should not be poisoned")
            .push((RetryCallbackPhase::RuleDecision, context.total_elapsed()));
        self.clock
            .advance(Duration::from_secs(1))
            .expect("callback elapsed clock should advance");
        assert!(!self.panic_after_advance, "elapsed rule panic");
        RetryDecision::Retry
    }
}

/// Creates empty shared storage for callback elapsed-time observations.
pub(crate) fn callback_elapsed_records() -> CallbackElapsedRecords {
    Arc::new(Mutex::new(Vec::new()))
}

/// Asserts one callback-panic terminal includes time consumed before panic.
pub(crate) fn assert_callback_panic_elapsed(
    error: &RetryError<TestError>,
    phase: RetryCallbackPhase,
) {
    let RetryFailure::CallbackFailed { callback, .. } = error.failure() else {
        panic!("expected a callback failure, got {:?}", error.failure());
    };
    let expected_kind = if phase == RetryCallbackPhase::RuleDecision {
        RetryCallbackKind::Rule
    } else {
        RetryCallbackKind::Observer
    };
    assert_eq!(callback.callback(), expected_kind);
    assert_eq!(callback.phase(), phase);
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(1));
}

/// Counts invocations of each observer phase independently.
#[derive(Default)]
pub(crate) struct ObserverPhaseCounts {
    started: AtomicUsize,
    failed: AtomicUsize,
    scheduled: AtomicUsize,
}

impl ObserverPhaseCounts {
    /// Returns the number of calls observed for `phase`.
    pub(crate) fn calls(&self, phase: RetryCallbackPhase) -> usize {
        match phase {
            RetryCallbackPhase::AttemptStarted => {
                self.started.load(Ordering::SeqCst)
            }
            RetryCallbackPhase::AttemptFailed => {
                self.failed.load(Ordering::SeqCst)
            }
            RetryCallbackPhase::RetryScheduled => {
                self.scheduled.load(Ordering::SeqCst)
            }
            RetryCallbackPhase::RuleDecision => 0,
        }
    }
}

/// Observer that panics at one selected lifecycle phase.
pub(crate) struct PanickingPhaseObserver {
    phase: RetryCallbackPhase,
}

impl PanickingPhaseObserver {
    /// Creates an observer that panics during `phase`.
    pub(crate) fn new(phase: RetryCallbackPhase) -> Self {
        Self { phase }
    }
}

impl RetryObserver<TestError> for PanickingPhaseObserver {
    /// Panics when attempt-started is the selected phase.
    fn on_attempt_started(&self, _context: &RetryContext) {
        if self.phase == RetryCallbackPhase::AttemptStarted {
            panic!("matrix observer panic");
        }
    }

    /// Panics when attempt-failed is the selected phase.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::AttemptFailed {
            panic!("matrix observer panic");
        }
    }

    /// Panics when retry-scheduled is the selected phase.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::RetryScheduled {
            panic!("matrix observer panic");
        }
    }
}

/// Observer that records each lifecycle phase reached after prior callbacks.
pub(crate) struct CountingPhaseObserver(pub(crate) Arc<ObserverPhaseCounts>);

impl RetryObserver<TestError> for CountingPhaseObserver {
    /// Records one attempt-started callback.
    fn on_attempt_started(&self, _context: &RetryContext) {
        self.0.started.fetch_add(1, Ordering::SeqCst);
    }

    /// Records one attempt-failed callback.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        self.0.failed.fetch_add(1, Ordering::SeqCst);
    }

    /// Records one retry-scheduled callback.
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        self.0.scheduled.fetch_add(1, Ordering::SeqCst);
    }
}

/// Asserts the complete first-attempt abort terminal shape.
pub(crate) fn assert_matrix_abort(error: &RetryError<TestError>) {
    let RetryFailure::Aborted { last_failure, .. } = error.failure() else {
        panic!("expected an aborted terminal failure");
    };
    assert_eq!(last_failure, &AttemptFailure::Error(TestError("matrix")));
    assert_eq!(error.failure().last_failure(), Some(last_failure));
    assert_eq!(error.failure().last_error(), Some(&TestError("matrix")));
    assert_eq!(error.last_error(), Some(&TestError("matrix")));
    assert_eq!(error.failure().to_string(), "retry aborted: matrix");
    assert_terminal_context(error.context(), 1, None);
}

/// Asserts an exhausted terminal shape and its retained application failure.
pub(crate) fn assert_matrix_limit(
    error: &RetryError<TestError>,
    limit: RetryLimitKind,
    expected_attempts: u32,
    has_last_failure: bool,
) {
    let RetryFailure::Exhausted {
        limit: actual_limit,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected an exhausted terminal failure");
    };
    assert_eq!(*actual_limit, limit);
    if has_last_failure {
        assert_eq!(
            last_failure,
            &Some(AttemptFailure::Error(TestError("matrix")))
        );
    } else {
        assert_eq!(last_failure, &None);
    }
    assert_eq!(error.failure().last_failure(), last_failure.as_ref());
    assert_eq!(
        error.failure().last_error(),
        has_last_failure.then_some(&TestError("matrix"))
    );
    let suffix = if has_last_failure {
        "; last attempt failed: matrix"
    } else {
        ""
    };
    assert_eq!(
        error.failure().to_string(),
        format!("retry limit exhausted: {limit}{suffix}")
    );
    assert_terminal_context(error.context(), expected_attempts, None);
    match limit {
        RetryLimitKind::Attempts => {
            assert_eq!(error.context().max_attempts(), 1);
        }
        RetryLimitKind::OperationElapsed => {
            assert_eq!(
                error.context().max_operation_elapsed(),
                Some(Duration::from_secs(1))
            );
            assert_eq!(
                error.context().operation_elapsed(),
                Duration::from_secs(1)
            );
            assert_eq!(
                error.context().last_attempt_elapsed(),
                Duration::from_secs(1)
            );
        }
        RetryLimitKind::TotalElapsed => {
            assert_eq!(
                error.context().max_total_elapsed(),
                Some(Duration::from_secs(1))
            );
            assert_eq!(error.context().total_elapsed(), Duration::from_secs(1));
        }
    }
}

/// Asserts structured rule-panic attribution and retained attempt data.
pub(crate) fn assert_matrix_rule_panic(
    error: &RetryError<TestError>,
    later_rule_calls: &AtomicUsize,
) {
    assert_matrix_callback(
        error,
        RetryCallbackKind::Rule,
        RetryCallbackPhase::RuleDecision,
        true,
        1,
        Some(1),
    );
    assert_eq!(later_rule_calls.load(Ordering::SeqCst), 0);
}

/// Asserts observer-panic attribution, callback short-circuiting, and context.
pub(crate) fn assert_matrix_observer_panic(
    error: &RetryError<TestError>,
    phase: RetryCallbackPhase,
    later_counts: &ObserverPhaseCounts,
) {
    let (has_last_failure, attempts, current_attempt) = match phase {
        RetryCallbackPhase::AttemptStarted => (false, 0, Some(1)),
        RetryCallbackPhase::AttemptFailed
        | RetryCallbackPhase::RetryScheduled => (true, 1, Some(1)),
        RetryCallbackPhase::RuleDecision => {
            panic!("rule decision is not an observer phase")
        }
    };
    assert_matrix_callback(
        error,
        RetryCallbackKind::Observer,
        phase,
        has_last_failure,
        attempts,
        current_attempt,
    );
    assert_eq!(later_counts.calls(phase), 0);
    if phase == RetryCallbackPhase::RetryScheduled {
        assert_eq!(error.context().next_delay(), Some(Duration::ZERO));
        assert_eq!(error.context().retry_after_hint(), None);
    }
}

/// Asserts a clock or timer infrastructure terminal and its context scope.
pub(crate) fn assert_matrix_infrastructure(
    error: &RetryError<TestError>,
    expected: &str,
    expected_attempts: u32,
    current_attempt: Option<u32>,
    has_last_failure: bool,
) {
    let RetryFailure::Infrastructure {
        failure,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected an infrastructure terminal failure");
    };
    match (expected, failure) {
        ("clock", RetryInfrastructureFailure::Clock { message })
        | ("timer", RetryInfrastructureFailure::Timer { message }) => {
            assert!(!message.is_empty());
        }
        _ => panic!("unexpected infrastructure failure: {failure:?}"),
    }
    assert_eq!(
        last_failure.as_ref(),
        has_last_failure.then_some(&AttemptFailure::Error(TestError("matrix")))
    );
    assert_eq!(error.failure().last_failure(), last_failure.as_ref());
    assert_eq!(
        error.failure().last_error(),
        has_last_failure.then_some(&TestError("matrix"))
    );
    let suffix = if has_last_failure {
        "; last attempt failed: matrix"
    } else {
        ""
    };
    assert_eq!(
        error.failure().to_string(),
        format!("retry infrastructure failed: {failure}{suffix}")
    );
    assert_terminal_context(
        error.context(),
        expected_attempts,
        current_attempt,
    );
}

/// Asserts timeout terminal and attempt-failure scopes remain identical.
pub(crate) fn assert_matrix_timeout(
    error: &RetryError<TestError>,
    scope: RetryTimeoutScope,
    expected_attempts: u32,
) {
    let RetryFailure::TimedOut {
        scope: terminal_scope,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a timeout terminal failure");
    };
    assert_eq!(*terminal_scope, scope);
    assert_eq!(last_failure, &Some(AttemptFailure::TimedOut { scope }));
    assert_eq!(error.failure().last_failure(), last_failure.as_ref());
    assert_eq!(error.failure().last_error(), None);
    assert_eq!(
        error.failure().to_string(),
        format!(
            "retry timed out: {scope}; last attempt failed: {}",
            last_failure
                .as_ref()
                .expect("timeout matrix retains its failed attempt")
        )
    );
    assert_terminal_context(error.context(), expected_attempts, None);
}

/// Creates a timer whose completion sample regresses behind flow start.
pub(crate) fn completion_regressing_timer() -> Arc<dyn Timer> {
    Arc::new(CompletionRegressingTimer {
        clock: CompletionRegressingClock {
            domain: ClockDomain::new(),
            samples: AtomicUsize::new(0),
        },
    })
}

/// Creates a timer whose post-rule terminal refresh regresses behind flow
/// start.
pub(crate) fn rule_terminal_regressing_timer() -> Arc<dyn Timer> {
    Arc::new(RuleTerminalRegressingTimer {
        clock: RuleTerminalRegressingClock {
            domain: ClockDomain::new(),
            samples: AtomicUsize::new(0),
        },
    })
}

/// Asserts shared terminal context fields used throughout the facade matrix.
fn assert_terminal_context(
    context: &RetryContext,
    attempts: u32,
    current_attempt: Option<u32>,
) {
    assert_eq!(context.attempts(), attempts);
    assert_eq!(
        context.current_attempt().map(std::num::NonZeroU32::get),
        current_attempt
    );
    if current_attempt.is_none() {
        assert_eq!(context.current_attempt_timeout(), None);
    }
}

/// Asserts one structured callback failure including its retained panic text.
fn assert_matrix_callback(
    error: &RetryError<TestError>,
    kind: RetryCallbackKind,
    phase: RetryCallbackPhase,
    has_last_failure: bool,
    attempts: u32,
    current_attempt: Option<u32>,
) {
    let RetryFailure::CallbackFailed {
        callback,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a callback terminal failure");
    };
    assert_eq!(callback.callback(), kind);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), phase);
    let expected_message = if kind == RetryCallbackKind::Rule {
        "matrix rule panic"
    } else {
        "matrix observer panic"
    };
    assert_eq!(callback.panic().message(), Some(expected_message));
    assert_eq!(
        last_failure.as_ref(),
        has_last_failure.then_some(&AttemptFailure::Error(TestError("matrix")))
    );
    assert_eq!(error.failure().last_failure(), last_failure.as_ref());
    assert_eq!(
        error.failure().last_error(),
        has_last_failure.then_some(&TestError("matrix"))
    );
    let suffix = if has_last_failure {
        "; last attempt failed: matrix"
    } else {
        ""
    };
    assert_eq!(
        error.failure().to_string(),
        format!("retry callback failed: {callback}{suffix}")
    );
    assert_terminal_context(error.context(), attempts, current_attempt);
}

/// Clock that returns a regressing fifth sample for completion handling.
struct CompletionRegressingClock {
    domain: ClockDomain,
    samples: AtomicUsize,
}

impl MonotonicClock for CompletionRegressingClock {
    /// Returns the stable clock domain.
    fn domain(&self) -> ClockDomain {
        self.domain
    }

    /// Returns a regressing sample after admission and attempt commitment.
    fn now(&self) -> MonotonicInstant {
        let sample = self.samples.fetch_add(1, Ordering::SeqCst);
        let elapsed = if sample < 4 {
            Duration::from_secs(1)
        } else {
            Duration::ZERO
        };
        MonotonicInstant::new(self.domain, elapsed)
    }

    /// This scripted clock does not create nested timers.
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("the matrix clock does not create nested timers")
    }
}

/// Timer exposing the completion-regressing clock to every facade.
struct CompletionRegressingTimer {
    clock: CompletionRegressingClock,
}

/// Clock that regresses only after a rule has selected a terminal result.
struct RuleTerminalRegressingClock {
    domain: ClockDomain,
    samples: AtomicUsize,
}

impl MonotonicClock for RuleTerminalRegressingClock {
    /// Returns the stable clock domain.
    fn domain(&self) -> ClockDomain {
        self.domain
    }

    /// Returns a regressing seventh sample after rule evaluation completes.
    fn now(&self) -> MonotonicInstant {
        let sample = self.samples.fetch_add(1, Ordering::SeqCst);
        let elapsed = if sample < 6 {
            Duration::from_secs(1)
        } else {
            Duration::ZERO
        };
        MonotonicInstant::new(self.domain, elapsed)
    }

    /// This scripted clock does not create nested timers.
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("the matrix clock does not create nested timers")
    }
}

/// Timer exposing [`RuleTerminalRegressingClock`] to retry facades.
struct RuleTerminalRegressingTimer {
    clock: RuleTerminalRegressingClock,
}

impl Timer for RuleTerminalRegressingTimer {
    /// Returns the scripted clock.
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }

    /// Returns an immediately ready timer after validating its clock domain.
    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        deadline.validate_domain(self.clock.domain())?;
        Ok(Box::pin(async { Ok(()) }))
    }
}

impl Timer for CompletionRegressingTimer {
    /// Returns the scripted clock.
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }

    /// Returns an immediately ready timer after validating its clock domain.
    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        deadline.validate_domain(self.clock.domain())?;
        Ok(Box::pin(async { Ok(()) }))
    }
}
