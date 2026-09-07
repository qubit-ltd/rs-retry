// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::num::NonZeroU32;

use qubit_retry::AttemptFailure;
use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryPanic;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

use crate::support::UnitTestError;

/// Verifies public component constructors expose their complete stable data.
#[test]
fn test_public_failure_component_constructors_and_accessors() {
    let context = RetryContext::new(2, 4);
    assert_eq!(context.attempts(), 2);
    assert_eq!(context.current_attempt().map(NonZeroU32::get), Some(2));
    assert_eq!(context.max_attempts(), 4);

    let callback = RetryCallbackFailure::new(
        RetryCallbackKind::Observer,
        3,
        RetryCallbackPhase::RetryScheduled,
        RetryPanic::String("observer panic".to_owned()),
    );
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 3);
    assert_eq!(callback.phase(), RetryCallbackPhase::RetryScheduled);
    assert_eq!(callback.panic().message(), Some("observer panic"));

    let attempt = AttemptFailure::<UnitTestError>::TimedOut {
        scope: RetryTimeoutScope::Flow,
    };
    assert!(attempt.is_timeout());
    assert_eq!(attempt.timeout_scope(), Some(RetryTimeoutScope::Flow));
    assert_eq!(attempt.as_error(), None);

    let infrastructure = RetryInfrastructureFailure::WorkerStillRunning {
        trigger: WorkerStopTrigger::Cancellation,
    };
    assert_eq!(infrastructure.message(), None);
    assert_eq!(
        infrastructure.worker_stop_trigger(),
        Some(WorkerStopTrigger::Cancellation)
    );

    let cancellation = RetryCancellationToken::new();
    assert!(!cancellation.is_cancelled());
    cancellation.cancel();
    assert!(cancellation.is_cancelled());
}
