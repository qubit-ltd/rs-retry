// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public cancellation-phase behavior.

use qubit_retry::RetryCancellationPhase;

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
