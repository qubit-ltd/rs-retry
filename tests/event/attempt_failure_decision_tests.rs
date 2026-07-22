// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::AttemptFailureDecision;

/// Verifies the default failure decision delegates to the retry policy.
#[test]
fn test_attempt_failure_decision_default_uses_policy_default() {
    assert_eq!(
        AttemptFailureDecision::default(),
        AttemptFailureDecision::UseDefault
    );
}

/// Verifies RetryAfter serde stores half-up rounded milliseconds.
#[test]
fn test_attempt_failure_decision_retry_after_serde_rounds_milliseconds() {
    let decision =
        AttemptFailureDecision::RetryAfter(Duration::from_micros(1500));
    let json = serde_json::to_string(&decision)
        .expect("retry-after decision should serialize");

    assert_eq!(json, r#"{"RetryAfter":2}"#);
    let decoded: AttemptFailureDecision = serde_json::from_str(&json)
        .expect("retry-after decision should deserialize");
    assert_eq!(
        decoded,
        AttemptFailureDecision::RetryAfter(Duration::from_millis(2))
    );
}
