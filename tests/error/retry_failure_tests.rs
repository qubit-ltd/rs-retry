// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryPanic;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

/// Verifies retry-limit classifications have stable diagnostic names.
#[test]
fn test_retry_limit_kind_display() {
    let cases = [
        (RetryLimitKind::Attempts, "attempts"),
        (RetryLimitKind::OperationElapsed, "operation elapsed"),
        (RetryLimitKind::TotalElapsed, "total elapsed"),
    ];
    for (kind, expected) in cases {
        assert_eq!(kind.to_string(), expected);
    }
}

/// Verifies timeout scopes have stable diagnostic names.
#[test]
fn test_retry_timeout_scope_display() {
    let cases = [
        (RetryTimeoutScope::Attempt, "attempt"),
        (RetryTimeoutScope::Flow, "flow"),
    ];
    for (scope, expected) in cases {
        assert_eq!(scope.to_string(), expected);
    }
}

/// Verifies cancellation phases have stable diagnostic names.
#[test]
fn test_retry_cancellation_phase_display() {
    let cases = [
        (RetryCancellationPhase::BeforeAttempt, "before attempt"),
        (RetryCancellationPhase::Attempt, "attempt"),
        (RetryCancellationPhase::Backoff, "backoff"),
    ];
    for (phase, expected) in cases {
        assert_eq!(phase.to_string(), expected);
    }
}

/// Verifies callback classifications have stable diagnostic names.
#[test]
fn test_retry_callback_kind_display() {
    let cases = [
        (RetryCallbackKind::Rule, "rule"),
        (RetryCallbackKind::Observer, "observer"),
    ];
    for (kind, expected) in cases {
        assert_eq!(kind.to_string(), expected);
    }
}

/// Verifies callback phases have stable diagnostic names.
#[test]
fn test_retry_callback_phase_display() {
    let cases = [
        (RetryCallbackPhase::AttemptStarted, "attempt started"),
        (RetryCallbackPhase::AttemptFailed, "attempt failed"),
        (RetryCallbackPhase::RuleDecision, "rule decision"),
        (RetryCallbackPhase::RetryScheduled, "retry scheduled"),
    ];
    for (phase, expected) in cases {
        assert_eq!(phase.to_string(), expected);
    }
}

/// Verifies panic payloads preserve string text and classify other payloads.
#[test]
fn test_retry_panic_accessors_and_display() {
    let static_text = RetryPanic::StaticStr("static panic");
    let owned_text = RetryPanic::String("owned panic".to_owned());
    let non_string = RetryPanic::NonString;

    assert_eq!(static_text.message(), Some("static panic"));
    assert_eq!(static_text.to_string(), "static panic");
    assert_eq!(owned_text.message(), Some("owned panic"));
    assert_eq!(owned_text.to_string(), "owned panic");
    assert_eq!(non_string.message(), None);
    assert_eq!(non_string.to_string(), "non-string panic payload");
}

/// Verifies callback failures expose complete callback attribution.
#[test]
fn test_retry_callback_failure_accessors_and_display() {
    let failure = RetryCallbackFailure::new(
        RetryCallbackKind::Observer,
        2,
        RetryCallbackPhase::AttemptFailed,
        RetryPanic::StaticStr("observer panic"),
    );

    assert_eq!(failure.callback(), RetryCallbackKind::Observer);
    assert_eq!(failure.index(), 2);
    assert_eq!(failure.phase(), RetryCallbackPhase::AttemptFailed);
    assert_eq!(failure.panic().message(), Some("observer panic"));
    assert_eq!(
        failure.to_string(),
        "observer callback 2 panicked during attempt failed: observer panic"
    );
}

/// Verifies worker-stop triggers have stable diagnostic names.
#[test]
fn test_worker_stop_trigger_display() {
    let cases = [
        (WorkerStopTrigger::AttemptTimeout, "attempt timeout"),
        (WorkerStopTrigger::FlowTimeout, "flow timeout"),
        (WorkerStopTrigger::Cancellation, "cancellation"),
    ];
    for (trigger, expected) in cases {
        assert_eq!(trigger.to_string(), expected);
    }
}

/// Verifies infrastructure failures retain messages and stop triggers.
#[test]
fn test_retry_infrastructure_failure_accessors_and_display() {
    let cases = [
        (
            RetryInfrastructureFailure::Clock {
                message: "clock unavailable".into(),
            },
            Some("clock unavailable"),
            None,
            "clock failed: clock unavailable",
        ),
        (
            RetryInfrastructureFailure::Timer {
                message: "timer unavailable".into(),
            },
            Some("timer unavailable"),
            None,
            "timer failed: timer unavailable",
        ),
        (
            RetryInfrastructureFailure::WorkerSpawn {
                message: "worker unavailable".into(),
            },
            Some("worker unavailable"),
            None,
            "worker spawn failed: worker unavailable",
        ),
        (
            RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::FlowTimeout,
            },
            None,
            Some(WorkerStopTrigger::FlowTimeout),
            "worker still running after flow timeout",
        ),
    ];

    for (failure, message, trigger, expected) in cases {
        assert_eq!(failure.message(), message);
        assert_eq!(failure.worker_stop_trigger(), trigger);
        assert_eq!(failure.to_string(), expected);
    }
}
