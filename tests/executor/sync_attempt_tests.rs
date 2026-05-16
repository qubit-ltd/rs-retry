/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::{
    AttemptFailure,
    AttemptFailureDecision,
    Retry,
    RetryContext,
};

/// Verifies sync attempt failures are observable through the public listener API.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_sync_attempt_failure_is_observable_through_failure_listener() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .on_failure(
            |failure: &AttemptFailure<&'static str>, context: &RetryContext| {
                assert_eq!(1, context.attempt());
                assert_eq!(Some(&"boom"), failure.as_error());
                AttemptFailureDecision::Abort
            },
        )
        .build()
        .unwrap();

    assert!(
        retry
            .run(|| -> Result<(), &'static str> { Err("boom") })
            .is_err()
    );
}
