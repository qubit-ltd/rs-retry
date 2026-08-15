// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public callback-phase behavior.

use qubit_retry::RetryCallbackPhase;

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
