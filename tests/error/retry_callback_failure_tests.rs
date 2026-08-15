// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public callback-failure behavior.

use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryPanic;

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
