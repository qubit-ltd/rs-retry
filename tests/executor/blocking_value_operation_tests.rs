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
    AttemptCancelToken,
    Retry,
};

use crate::support::TestError;

/// Non-clone value used to verify worker value capture.
#[derive(Debug, PartialEq, Eq)]
struct NonCloneValue {
    /// Captured value text.
    text: &'static str,
}

/// Verifies blocking worker value capture through the public retry API.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_blocking_value_operation_is_observable_through_non_clone_success_value() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let value = retry
        .run_in_worker(|_token: AttemptCancelToken| {
            Ok::<_, TestError>(NonCloneValue { text: "ok" })
        })
        .expect("worker operation should succeed");

    assert_eq!(value, NonCloneValue { text: "ok" });
}
