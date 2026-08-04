// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryErrorReason;

#[test]
fn test_retry_error_reason_serializes_and_compares_terminal_reasons() {
    let reasons = [
        RetryErrorReason::Aborted,
        RetryErrorReason::AttemptsExceeded,
        RetryErrorReason::MaxOperationElapsedExceeded,
        RetryErrorReason::MaxTotalElapsedExceeded,
        RetryErrorReason::UnsupportedOperation,
        RetryErrorReason::SleeperFailed,
        RetryErrorReason::WorkerStillRunning,
    ];

    assert_eq!(7, reasons.len());
    assert_eq!(
        "\"AttemptsExceeded\"",
        serde_json::to_string(&RetryErrorReason::AttemptsExceeded).unwrap(),
    );
    assert_eq!(
        RetryErrorReason::WorkerStillRunning,
        serde_json::from_str("\"WorkerStillRunning\"").unwrap(),
    );
}

/// Verifies terminal reasons expose stable semantic classifications.
#[test]
fn test_retry_error_reason_semantic_classifications() {
    assert!(RetryErrorReason::MaxOperationElapsedExceeded.is_elapsed_limit());
    assert!(RetryErrorReason::MaxTotalElapsedExceeded.is_elapsed_limit());
    assert!(!RetryErrorReason::AttemptsExceeded.is_elapsed_limit());

    assert!(RetryErrorReason::SleeperFailed.is_infrastructure_failure());
    assert!(RetryErrorReason::WorkerStillRunning.is_infrastructure_failure());
    assert!(!RetryErrorReason::Aborted.is_infrastructure_failure());

    assert!(RetryErrorReason::UnsupportedOperation.is_unsupported_operation());
    assert!(!RetryErrorReason::SleeperFailed.is_unsupported_operation());
}
