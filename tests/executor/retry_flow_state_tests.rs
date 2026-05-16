/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::sync::{
    Arc,
    Mutex,
};

use qubit_retry::{
    Retry,
    RetryContext,
    RetryErrorReason,
};

/// Verifies retry-flow state attempt counting through public retry contexts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_flow_state_counts_attempts_and_preserves_last_failure() {
    let before_attempts = Arc::new(Mutex::new(Vec::new()));
    let captured_before_attempts = Arc::clone(&before_attempts);
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .no_delay()
        .before_attempt(move |context: &RetryContext| {
            captured_before_attempts
                .lock()
                .expect("attempt list should be lockable")
                .push(context.attempt());
        })
        .build()
        .expect("retry should build");
    let mut calls = 0;

    let error = retry
        .run(|| -> Result<(), &'static str> {
            calls += 1;
            Err("always")
        })
        .expect_err("retry should exhaust attempts");

    assert_eq!(2, calls);
    assert_eq!(RetryErrorReason::AttemptsExceeded, error.reason());
    assert_eq!(2, error.attempts());
    assert_eq!(Some(&"always"), error.last_error());
    assert_eq!(
        vec![1, 2],
        *before_attempts
            .lock()
            .expect("attempt list should be lockable")
    );
}
