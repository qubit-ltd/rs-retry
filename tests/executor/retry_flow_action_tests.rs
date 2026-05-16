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
    RetryErrorReason,
};

/// Verifies retry and terminal flow branches through public retry behavior.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_flow_action_paths_cover_retry_and_finished_results() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(3)
        .no_delay()
        .on_failure(
            |_failure: &AttemptFailure<&'static str>, context: &RetryContext| {
                if context.attempt() == 1 {
                    AttemptFailureDecision::UseDefault
                } else {
                    AttemptFailureDecision::Abort
                }
            },
        )
        .build()
        .unwrap();
    let mut attempts = 0;

    let error = retry
        .run(|| -> Result<(), &'static str> {
            attempts += 1;
            Err("fail")
        })
        .unwrap_err();

    assert_eq!(2, attempts);
    assert_eq!(RetryErrorReason::Aborted, error.reason());
}
