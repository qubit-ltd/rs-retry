// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Compares owned single-step execution with the synchronous retry facade.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_clock::ClockDomain;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryObserver;
use qubit_retry::RetrySession;
use qubit_retry::RetrySessionAdmission;
use qubit_retry::RetrySessionStep;

use crate::support::FixedRetryRandomSource;

/// Mutable clock permits deterministic time advancement and invalid samples.
struct SessionClock(Mutex<MonotonicInstant>);

impl MonotonicClock for SessionClock {
    /// Returns the currently injected domain.
    fn domain(&self) -> ClockDomain {
        self.now().domain()
    }
    /// Returns the current scripted sample.
    fn now(&self) -> MonotonicInstant {
        *self.0.lock().expect("clock lock")
    }
    /// Rejects nested timers; tests use the enclosing timer.
    fn new_timer(&self) -> Arc<dyn Timer> {
        panic!("use the enclosing test timer")
    }
}

/// A synchronous wait advances time immediately or requests cancellation.
struct SessionTimer {
    clock: SessionClock,
    registrations: AtomicUsize,
    cancel_on_wait: Option<RetryCancellationToken>,
}

impl SessionTimer {
    /// Constructs a deterministic timer starting at one second.
    fn new(cancel_on_wait: Option<RetryCancellationToken>) -> Self {
        Self {
            clock: SessionClock(Mutex::new(MonotonicInstant::new(
                ClockDomain::new(),
                Duration::from_secs(1),
            ))),
            registrations: AtomicUsize::new(0),
            cancel_on_wait,
        }
    }

    /// Drives the external scheduler to its due time, or cancels its wait.
    fn elapse(&self, deadline: MonotonicInstant) {
        if let Some(token) = &self.cancel_on_wait {
            token.cancel();
        } else {
            *self.clock.0.lock().expect("clock lock") = deadline;
        }
    }
}

impl Timer for SessionTimer {
    /// Borrows the deterministic clock.
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
    /// Simulates a registered wait after checking the deadline domain.
    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        deadline.validate_domain(self.clock.domain())?;
        self.registrations.fetch_add(1, Ordering::SeqCst);
        self.elapse(deadline);
        Ok(Box::pin(async { Ok(()) }))
    }
}

/// Records callback order and selected delay without depending on wall time.
struct SessionObserver(Arc<Mutex<Vec<String>>>);

impl RetryObserver<&'static str> for SessionObserver {
    /// Records the upcoming operation ordinal.
    fn on_before_attempt(&self, context: &RetryContext) {
        self.0.lock().expect("event lock").push(format!(
            "before:{}",
            context.current_attempt().expect("ordinal")
        ));
    }
    /// Records committed failure notification.
    fn on_attempt_failed(&self, _: &AttemptFailure<&'static str>, context: &RetryContext) {
        self.0
            .lock()
            .expect("event lock")
            .push(format!("failed:{}", context.attempts()));
    }
    /// Records the selected effective delay.
    fn on_retry_scheduled(&self, backoff: &BackoffStep, _: &RetryContext) {
        self.0
            .lock()
            .expect("event lock")
            .push(format!("scheduled:{:?}", backoff.effective_delay()));
    }
    /// Records successful completion.
    fn on_success(&self, _: &RetryContext) {
        self.0.lock().expect("event lock").push("success".into());
    }
    /// Records the structured terminal classification.
    fn on_terminal_failure(&self, reason: &RetryErrorReason, _: &RetryContext) {
        self.0
            .lock()
            .expect("event lock")
            .push(format!("terminal:{}", summarize_reason(reason)));
    }
}

/// Compares structured clock failures without process-local domain identifiers.
fn summarize_reason(reason: &RetryErrorReason) -> String {
    match reason {
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Clock { .. },
        } => "infrastructure clock".to_owned(),
        _ => reason.to_string(),
    }
}

/// Runs a scenario through either facade and returns its observable contract.
fn run_scenario(session: bool, scenario: &str) -> (u32, String, Vec<String>) {
    let token = RetryCancellationToken::new();
    let timer = Arc::new(SessionTimer::new(
        (scenario == "cancel").then(|| token.clone()),
    ));
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = RetryConfig::builder()
        .max_attempts(3)
        .fallback(RetryFallback::Retry)
        .backoff(if scenario == "zero" {
            BackoffPolicy::immediate()
        } else {
            BackoffPolicy::fixed(Duration::from_secs(2)).with_full_jitter()
        })
        .observer(SessionObserver(Arc::clone(&events)));
    if scenario == "permanent" {
        builder = builder.fallback(RetryFallback::Abort);
    }
    if scenario == "rule" {
        builder = builder.fallback(RetryFallback::Abort).rule(
            |_: &AttemptFailure<&'static str>, _: &RetryContext| {
                RetryDecision::RetryWithHint(Duration::from_secs(3))
            },
        );
    }
    if scenario == "budget" {
        builder = builder.total_time_budget(Duration::from_secs(2));
    }
    let config = builder.build().expect("valid config");
    let mut attempts = 0;
    let mut operation = || {
        attempts += 1;
        if scenario == "regress" {
            *timer.clock.0.lock().expect("clock lock") =
                MonotonicInstant::new(timer.clock.domain(), Duration::ZERO);
        }
        if scenario == "domain" {
            *timer.clock.0.lock().expect("clock lock") =
                MonotonicInstant::new(ClockDomain::new(), Duration::from_secs(1));
        }
        if scenario == "success"
            || scenario == "regress"
            || scenario == "domain"
            || (attempts == 2 && scenario != "exhaust" && scenario != "budget")
        {
            Ok(42)
        } else {
            Err("busy")
        }
    };
    let result = if session {
        let mut retry = RetrySession::new_with_random_source(
            config,
            timer.clone(),
            Arc::new(FixedRetryRandomSource::new(0.5)),
        )
        .with_cancellation_token(token);
        let mut next_ordinal = 1;
        loop {
            let ordinal = match retry.begin_attempt() {
                Ok(RetrySessionAdmission::Admitted(ordinal)) => ordinal,
                Ok(RetrySessionAdmission::Waiting(due)) => {
                    timer.elapse(due);
                    continue;
                }
                Err(error) => break Err(error),
            };
            let value = operation();
            assert_eq!(ordinal.get(), next_ordinal);
            next_ordinal += 1;
            let before = timer.clock.now();
            match retry.record_result(value) {
                RetrySessionStep::Complete(success) => break Ok(success),
                RetrySessionStep::Failed(error) => break Err(error),
                RetrySessionStep::RetryAt(due) => {
                    assert_eq!(due.domain(), timer.clock.domain());
                    let expected_delay = match scenario {
                        "rule" => Duration::from_secs(3),
                        "zero" => Duration::ZERO,
                        _ => Duration::from_secs(1),
                    };
                    assert_eq!(
                        due.duration_since(before).expect("same clock domain"),
                        expected_delay
                    );
                    assert_eq!(
                        timer.clock.now(),
                        before,
                        "record_result must not advance time"
                    );
                    assert_eq!(
                        timer.registrations.load(Ordering::SeqCst),
                        0,
                        "session must not register a timer"
                    );
                    timer.elapse(due);
                    if scenario == "budget" {
                        timer.elapse(
                            due.checked_add(Duration::from_secs(2))
                                .expect("valid instant"),
                        );
                    }
                }
            }
        }
    } else {
        Retry::new(&config)
            .timer(timer.clone())
            .random_source(Arc::new(FixedRetryRandomSource::new(0.5)))
            .cancellation_token(token)
            .run(operation)
    };
    let (count, reason) = match result {
        Ok(success) => {
            assert_eq!(*success.value(), 42);
            (success.context().attempts(), "success".to_owned())
        }
        Err(error) => (error.context().attempts(), summarize_reason(error.reason())),
    };
    assert_eq!(count, attempts);
    let records = events.lock().expect("event lock").clone();
    (count, reason, records)
}

/// Success, rules, exhaustion, clock faults, cancellation and jitter stay
/// aligned.
#[test]
fn test_retry_session_matches_retry_run() {
    for (scenario, expected_attempts, expected_reason) in [
        ("success", 1, "success"),
        ("permanent", 1, "aborted"),
        ("rule", 2, "success"),
        ("exhaust", 3, "exhausted (attempts)"),
        ("cancel", 1, "cancelled (backoff)"),
        ("regress", 1, "infrastructure clock"),
        ("domain", 1, "infrastructure clock"),
        ("zero", 2, "success"),
        ("jitter", 2, "success"),
    ] {
        let actual = run_scenario(true, scenario);
        assert_eq!(
            actual,
            run_scenario(false, scenario),
            "scenario: {scenario}"
        );
        let (attempts, reason, _) = actual;
        assert_eq!(attempts, expected_attempts, "scenario: {scenario}");
        assert_eq!(reason, expected_reason, "scenario: {scenario}");
    }
}

/// A waiting session must recheck elapsed limits before admitting new work.
#[test]
fn test_retry_session_rechecks_budget_after_wait() {
    let (attempts, reason, events) = run_scenario(true, "budget");
    assert_eq!(attempts, 1);
    assert_eq!(reason, "exhausted (total elapsed)");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("before:"))
            .count(),
        1
    );
}

/// Repeated early admission checks preserve the pending deadline and callbacks.
#[test]
fn test_retry_session_early_admission_waits_until_due() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = RetryConfig::builder()
        .max_attempts(2)
        .fallback(RetryFallback::Retry)
        .backoff(BackoffPolicy::fixed(Duration::from_secs(1)))
        .observer(SessionObserver(Arc::clone(&events)))
        .build()
        .expect("valid config");
    let timer = Arc::new(SessionTimer::new(None));
    let mut session = RetrySession::new(config, timer.clone());

    assert!(
        matches!(session.begin_attempt(), Ok(RetrySessionAdmission::Admitted(ordinal)) if ordinal.get() == 1)
    );
    let RetrySessionStep::RetryAt(due) = session.record_result::<()>(Err("busy")) else {
        panic!("retry should be scheduled");
    };
    assert!(
        matches!(session.begin_attempt(), Ok(RetrySessionAdmission::Waiting(waiting)) if waiting == due)
    );
    assert!(
        matches!(session.begin_attempt(), Ok(RetrySessionAdmission::Waiting(waiting)) if waiting == due)
    );
    assert_eq!(
        timer.clock.now(),
        MonotonicInstant::new(timer.clock.domain(), Duration::from_secs(1))
    );
    assert_eq!(
        events
            .lock()
            .expect("event lock")
            .iter()
            .filter(|event| event.starts_with("before:"))
            .count(),
        1
    );

    timer.elapse(due);
    assert!(
        matches!(session.begin_attempt(), Ok(RetrySessionAdmission::Admitted(ordinal)) if ordinal.get() == 2)
    );
    assert_eq!(
        events
            .lock()
            .expect("event lock")
            .iter()
            .filter(|event| event.starts_with("before:"))
            .count(),
        2
    );
}

/// Waiting admission rejects regressing and foreign-domain clock samples.
#[test]
fn test_retry_session_waiting_rejects_invalid_clock_samples() {
    for foreign_domain in [false, true] {
        let timer = Arc::new(SessionTimer::new(None));
        let config = RetryConfig::builder()
            .max_attempts(2)
            .fallback(RetryFallback::Retry)
            .backoff(BackoffPolicy::fixed(Duration::from_secs(1)))
            .build()
            .expect("valid config");
        let mut session = RetrySession::new(config, timer.clone());
        assert!(matches!(
            session.begin_attempt(),
            Ok(RetrySessionAdmission::Admitted(_))
        ));
        assert!(matches!(
            session.record_result::<()>(Err("busy")),
            RetrySessionStep::RetryAt(_)
        ));

        let invalid_now = if foreign_domain {
            MonotonicInstant::new(ClockDomain::new(), Duration::from_secs(1))
        } else {
            MonotonicInstant::new(timer.clock.domain(), Duration::ZERO)
        };
        *timer.clock.0.lock().expect("clock lock") = invalid_now;
        let error = session.begin_attempt().expect_err("invalid waiting clock");
        assert!(matches!(
            error.reason(),
            RetryErrorReason::Infrastructure {
                failure: RetryInfrastructureFailure::Clock { .. }
            }
        ));
    }
}

/// The session owns all configuration and can be moved into a delivery owner.
#[test]
fn test_retry_session_owns_config_and_accepts_non_clone_errors() {
    #[derive(Debug)]
    struct NonCloneError;
    let mut session = {
        let config = RetryConfig::<NonCloneError>::builder()
            .build()
            .expect("valid config");
        RetrySession::new(config, Arc::new(SessionTimer::new(None)))
    };
    let RetrySessionAdmission::Admitted(ordinal) = session.begin_attempt().expect("admitted")
    else {
        panic!("fresh session cannot wait");
    };
    assert_eq!(ordinal.get(), 1);
    assert!(matches!(
        session.record_result(Ok(7)),
        RetrySessionStep::Complete(_)
    ));
}

/// Admission cancellation notifies completion once without admitting work.
#[test]
fn test_retry_session_cancellation_before_admission_is_terminal() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = RetryConfig::builder()
        .observer(SessionObserver(Arc::clone(&events)))
        .build()
        .expect("valid config");
    let token = RetryCancellationToken::new();
    token.cancel();
    let mut session =
        RetrySession::new(config, Arc::new(SessionTimer::new(None))).with_cancellation_token(token);
    let error = session
        .begin_attempt()
        .expect_err("cancelled before admission");
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(
        summarize_reason(error.reason()),
        "cancelled (before attempt)"
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = session.begin_attempt();
        }))
        .is_err()
    );
    assert_eq!(
        *events.lock().expect("event lock"),
        ["terminal:cancelled (before attempt)"]
    );
}

/// Invalid call order cannot admit two operations or emit duplicate completion.
#[test]
fn test_retry_session_enforces_one_outstanding_attempt() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = RetryConfig::builder()
        .observer(SessionObserver(Arc::clone(&events)))
        .build()
        .expect("valid config");
    let mut session = RetrySession::new(config, Arc::new(SessionTimer::new(None)));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.record_result(Ok(1))))
            .is_err()
    );
    assert!(
        matches!(session.begin_attempt().expect("admitted"), RetrySessionAdmission::Admitted(ordinal) if ordinal.get() == 1)
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = session.begin_attempt();
        }))
        .is_err()
    );
    assert!(matches!(
        session.record_result(Ok(1)),
        RetrySessionStep::Complete(_)
    ));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.record_result(Ok(1))))
            .is_err()
    );
    assert_eq!(*events.lock().expect("event lock"), ["before:1", "success"]);
}

#[test]
fn test_session_without_flow_timeout_reports_overflowing_backoff() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::MAX))
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let timer = Arc::new(SessionTimer::new(None));
    let mut session = RetrySession::new(config, timer);
    assert!(matches!(
        session.begin_attempt(),
        Ok(RetrySessionAdmission::Admitted(_))
    ));
    let RetrySessionStep::Failed(error) = session.record_result::<()>(Err("busy")) else {
        panic!("unrepresentable uncapped delay must fail");
    };
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::Clock { .. }
        }
    ));
}
