// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryErrorKind;
use qubit_retry::RetryErrorReason;

#[test]
fn test_retry_error_reason_maps_to_stable_kind() {
    assert_eq!(
        RetryErrorReason::AttemptsExhausted.kind(),
        RetryErrorKind::Exhausted
    );
    assert_eq!(
        RetryErrorReason::FlowTimedOut.kind(),
        RetryErrorKind::TimedOut
    );
    assert_eq!(
        RetryErrorReason::TimerFailed.kind(),
        RetryErrorKind::Infrastructure
    );
}
