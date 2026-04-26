/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{AttemptTimeoutOption, AttemptTimeoutPolicy};

/// Verifies timeout option constructors and accessors.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
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
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
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
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_timeout_option_serde_uses_milliseconds() {
    let option = AttemptTimeoutOption::abort(Duration::from_millis(25));
    let json = serde_json::to_string(&option).expect("timeout option should serialize");
    assert!(json.contains("\"timeout\":25"));
    assert!(json.contains("\"policy\":\"Abort\""));

    let decoded: AttemptTimeoutOption =
        serde_json::from_str(&json).expect("timeout option should deserialize");
    assert_eq!(decoded, option);
}
