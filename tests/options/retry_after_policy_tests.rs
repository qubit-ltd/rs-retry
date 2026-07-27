// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::str::FromStr;

use qubit_retry::RetryAfterPolicy;

/// Verifies the default policy preserves the historical replacement behavior.
#[test]
fn test_retry_after_policy_default_is_replace() {
    assert_eq!(RetryAfterPolicy::Replace, RetryAfterPolicy::default());
}

/// Verifies policies round-trip through their configuration representations.
#[test]
fn test_retry_after_policy_display_and_parse() {
    for (text, policy) in [
        ("replace", RetryAfterPolicy::Replace),
        (
            "at_least_configured_delay",
            RetryAfterPolicy::AtLeastConfiguredDelay,
        ),
    ] {
        assert_eq!(text, policy.to_string());
        assert_eq!(
            policy,
            RetryAfterPolicy::from_str(text).expect("policy should parse")
        );
    }
}

/// Verifies unsupported policy text returns a useful validation error.
#[test]
fn test_retry_after_policy_rejects_invalid_text() {
    let error = RetryAfterPolicy::from_str("shorter")
        .expect_err("unsupported policy should be rejected");

    assert!(error.contains("replace"));
    assert!(error.contains("at_least_configured_delay"));
}
