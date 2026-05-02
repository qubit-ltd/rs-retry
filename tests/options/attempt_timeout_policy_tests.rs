/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::AttemptTimeoutPolicy;

/// Verifies timeout policy defaults and display formatting.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_timeout_policy_default_and_display() {
    assert_eq!(AttemptTimeoutPolicy::default(), AttemptTimeoutPolicy::Retry);
    assert_eq!(AttemptTimeoutPolicy::Retry.to_string(), "retry");
    assert_eq!(AttemptTimeoutPolicy::Abort.to_string(), "abort");
}

/// Verifies timeout policy parsing accepts supported policy text.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_timeout_policy_from_str_accepts_supported_values() {
    assert_eq!(
        " retry "
            .parse::<AttemptTimeoutPolicy>()
            .expect("retry policy should parse"),
        AttemptTimeoutPolicy::Retry
    );
    assert_eq!(
        "ABORT"
            .parse::<AttemptTimeoutPolicy>()
            .expect("abort policy should parse"),
        AttemptTimeoutPolicy::Abort
    );
}

/// Verifies timeout policy parsing rejects unsupported policy text.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_timeout_policy_from_str_rejects_unsupported_values() {
    let error = "stop"
        .parse::<AttemptTimeoutPolicy>()
        .expect_err("unsupported policy should fail");

    assert!(error.contains("retry"));
    assert!(error.contains("abort"));
}
