// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public prepared-attempt timeout selection behavior.

use std::future::pending;
use std::time::Duration;

use qubit_retry::Retry;
use qubit_retry::RetryFailure;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;

/// Verifies admission selects the shorter flow timeout as the fixed boundary.
#[tokio::test]
async fn test_prepared_attempt_plan_selects_the_shorter_flow_timeout() {
    let error = Retry::<()>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .asynchronous()
    .attempt_timeout(Duration::from_secs(1))
    .flow_timeout(Duration::from_millis(1))
    .run(pending::<Result<(), ()>>)
    .await
    .expect_err("the flow timeout must terminate the pending operation");
    assert!(matches!(
        error.failure(),
        RetryFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
            ..
        }
    ));
}
