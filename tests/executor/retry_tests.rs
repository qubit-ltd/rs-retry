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
    Retry,
    RetryErrorReason,
};

/// Verifies `Retry::run` returns a successful value and exhaustion error.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_run_returns_value_and_exhaustion_error() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .no_delay()
        .build()
        .unwrap();
    let mut attempts = 0;

    let value = retry
        .run(|| {
            attempts += 1;
            if attempts == 2 {
                Ok("done")
            } else {
                Err("again")
            }
        })
        .unwrap();
    assert_eq!("done", value);

    let error = retry
        .run(|| -> Result<(), &'static str> { Err("always") })
        .unwrap_err();
    assert_eq!(RetryErrorReason::AttemptsExceeded, error.reason());
}
