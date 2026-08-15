// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptCancellationToken;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

/// Non-clone value used to verify worker value capture.
#[derive(Debug, PartialEq, Eq)]
struct NonCloneValue {
    /// Captured value text.
    text: &'static str,
}

/// Verifies blocking worker value capture through the public retry API.
#[test]
fn test_blocking_value_operation_is_observable_through_non_clone_success_value()
{
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("retry should build");
    let retry = Retry::<TestError>::builder(policy).build();

    let value = retry
        .worker()
        .run(|_token: AttemptCancellationToken| {
            Ok::<_, TestError>(NonCloneValue { text: "ok" })
        })
        .expect("worker operation should succeed");

    assert_eq!(value.into_value(), NonCloneValue { text: "ok" });
}
