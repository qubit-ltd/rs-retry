/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{RetryAttemptFailure, RetryError};

use crate::support::TestError;

/// Verifies retry error helper methods, display output, and source chaining.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when helper or formatting behavior is
/// incorrect.
#[test]
fn test_display_source_and_last_error_helpers_work() {
    let error = RetryError::AttemptsExceeded {
        attempts: 2,
        max_attempts: 2,
        elapsed: Duration::from_millis(3),
        last_failure: RetryAttemptFailure::Error(TestError("boom")),
    };

    assert_eq!(error.attempts(), 2);
    assert_eq!(error.elapsed(), Duration::from_millis(3));
    assert_eq!(error.last_error(), Some(&TestError("boom")));
    assert!(error.to_string().contains("boom"));
    assert_eq!(
        std::error::Error::source(&error)
            .expect("application error should be exposed as source")
            .to_string(),
        "boom"
    );

    let timeout = RetryError::<TestError>::MaxElapsedExceeded {
        attempts: 1,
        elapsed: Duration::from_millis(4),
        max_elapsed: Duration::from_millis(4),
        last_failure: Some(RetryAttemptFailure::AttemptTimeout {
            elapsed: Duration::from_millis(4),
            timeout: Duration::from_millis(2),
        }),
    };
    assert!(timeout.to_string().contains("timed out"));
    assert!(std::error::Error::source(&timeout).is_none());
    assert_eq!(timeout.into_last_error(), None);

    let aborted = RetryError::Aborted {
        attempts: 1,
        elapsed: Duration::from_millis(1),
        failure: RetryAttemptFailure::Error(TestError("fatal")),
    };
    assert_eq!(aborted.elapsed(), Duration::from_millis(1));
    assert!(aborted.to_string().contains("aborted"));

    let elapsed_without_failure = RetryError::<TestError>::MaxElapsedExceeded {
        attempts: 0,
        elapsed: Duration::ZERO,
        max_elapsed: Duration::ZERO,
        last_failure: None,
    };
    assert!(elapsed_without_failure.to_string().contains("max elapsed"));
}
