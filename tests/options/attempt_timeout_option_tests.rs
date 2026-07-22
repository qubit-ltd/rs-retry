// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::{
    AttemptTimeoutOption,
    AttemptTimeoutPolicy,
};

/// Verifies timeout option constructors and accessors.
#[test]
fn test_attempt_timeout_option_constructors_and_accessors() {
    let retry = AttemptTimeoutOption::retry(Duration::from_millis(10));
    assert_eq!(retry.timeout(), Duration::from_millis(10));
    assert_eq!(retry.policy(), AttemptTimeoutPolicy::Retry);

    let abort = AttemptTimeoutOption::abort(Duration::from_millis(20));
    assert_eq!(abort.timeout(), Duration::from_millis(20));
    assert_eq!(abort.policy(), AttemptTimeoutPolicy::Abort);

    let updated = retry.with_policy(AttemptTimeoutPolicy::Abort);
    assert_eq!(updated.timeout(), Duration::from_millis(10));
    assert_eq!(updated.policy(), AttemptTimeoutPolicy::Abort);
}

/// Verifies timeout option validation rejects zero duration.
#[test]
fn test_attempt_timeout_option_validate_rejects_zero_duration() {
    let valid = AttemptTimeoutOption::retry(Duration::from_millis(1));
    assert!(valid.validate().is_ok());

    let error = AttemptTimeoutOption::retry(Duration::ZERO)
        .validate()
        .expect_err("zero timeout should be rejected");
    assert!(error.contains("greater than zero"));
}

/// Verifies timeout option serde uses millisecond duration values.
#[test]
fn test_attempt_timeout_option_serde_uses_milliseconds() {
    let option = AttemptTimeoutOption::abort(Duration::from_millis(25));
    let json = serde_json::to_string(&option)
        .expect("timeout option should serialize");
    assert!(json.contains("\"timeout\":25"));
    assert!(json.contains("\"policy\":\"Abort\""));

    let decoded: AttemptTimeoutOption =
        serde_json::from_str(&json).expect("timeout option should deserialize");
    assert_eq!(decoded, option);
}

/// Verifies timeout serde quantizes sub-millisecond values half-up.
#[test]
fn test_attempt_timeout_option_serde_quantizes_sub_millisecond_values() {
    let cases = [
        (Duration::from_micros(499), 0_u64),
        (Duration::from_micros(500), 1_u64),
        (Duration::from_micros(1500), 2_u64),
    ];

    for (timeout, expected_millis) in cases {
        let original = AttemptTimeoutOption::abort(timeout);
        let json = serde_json::to_string(&original)
            .expect("timeout option should serialize");
        let decoded: AttemptTimeoutOption = serde_json::from_str(&json)
            .expect("timeout option should deserialize");
        assert_eq!(decoded.timeout(), Duration::from_millis(expected_millis));
        if expected_millis == 0 {
            assert!(decoded.validate().is_err());
        } else {
            assert!(decoded.validate().is_ok());
        }
    }
}
