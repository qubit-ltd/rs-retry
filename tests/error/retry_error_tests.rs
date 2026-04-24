/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use qubit_retry::{AttemptFailure, Retry, RetryErrorReason};

use crate::support::TestError;

/// Verifies retry errors preserve terminal reason, context, and last failure.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_error_preserves_reason_context_and_last_failure() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("single failing attempt should stop");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(error.context().max_attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("failed")));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Error(TestError("failed")))
    ));
    assert_eq!(error.into_last_error(), Some(TestError("failed")));
}
