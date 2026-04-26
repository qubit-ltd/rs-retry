/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use qubit_retry::{AttemptFailure, AttemptPanic};

use crate::support::TestError;

/// Verifies attempt failure accessors return application errors only for error variants.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when error accessors return wrong values.
#[test]
fn test_attempt_failure_error_accessors_distinguish_timeout() {
    let failure = AttemptFailure::Error(TestError("boom"));
    assert_eq!(failure.as_error(), Some(&TestError("boom")));
    assert_eq!(failure.as_panic(), None);
    assert_eq!(failure.into_error(), Some(TestError("boom")));

    let timeout = AttemptFailure::<TestError>::Timeout;
    assert_eq!(timeout.as_error(), None);
    assert_eq!(timeout.as_panic(), None);
    assert_eq!(timeout.into_error(), None);

    let panic = AttemptFailure::<TestError>::Panic(AttemptPanic::new("worker failed"));
    assert_eq!(panic.as_error(), None);
    assert_eq!(
        panic
            .as_panic()
            .expect("panic failure should expose captured panic")
            .message(),
        "worker failed"
    );
    assert_eq!(panic.into_error(), None);
}

/// Verifies attempt failure display output for error and timeout variants.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when display output changes unexpectedly.
#[test]
fn test_attempt_failure_display_formats_variants() {
    assert_eq!(
        AttemptFailure::Error(TestError("operation failed")).to_string(),
        "operation failed"
    );
    assert_eq!(
        AttemptFailure::<TestError>::Timeout.to_string(),
        "attempt timed out"
    );
    assert_eq!(
        AttemptFailure::<TestError>::Panic(AttemptPanic::new("worker failed")).to_string(),
        "attempt panicked: worker failed"
    );
}
