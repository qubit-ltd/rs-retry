// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptTimeoutSource;

#[test]
fn test_attempt_timeout_source_orders_and_serializes_sources() {
    assert!(AttemptTimeoutSource::Configured < AttemptTimeoutSource::MaxOperationElapsed);
    assert!(AttemptTimeoutSource::MaxOperationElapsed < AttemptTimeoutSource::MaxTotalElapsed);
    assert_eq!(
        "\"MaxTotalElapsed\"",
        serde_json::to_string(&AttemptTimeoutSource::MaxTotalElapsed).unwrap(),
    );
    assert_eq!(
        AttemptTimeoutSource::Configured,
        serde_json::from_str("\"Configured\"").unwrap(),
    );
}
