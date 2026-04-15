/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetryAttemptFailure;

use crate::support::TestError;

/// Verifies borrowed attempt failures preserve error and timeout metadata.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when borrowed failures differ from their
/// source failures.
#[test]
fn test_borrowed_attempt_failure_preserves_error_and_timeout_metadata() {
    let failure = RetryAttemptFailure::Error(TestError("borrowed"));
    assert!(matches!(
        &failure,
        RetryAttemptFailure::Error(TestError("borrowed"))
    ));

    let timeout = RetryAttemptFailure::<TestError>::AttemptTimeout {
        elapsed: Duration::from_millis(3),
        timeout: Duration::from_millis(2),
    };
    assert!(matches!(
        &timeout,
        RetryAttemptFailure::AttemptTimeout { elapsed, timeout }
            if *elapsed == Duration::from_millis(3) && *timeout == Duration::from_millis(2)
    ));
}
