// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::AttemptTimeoutPolicy;
use qubit_retry::RetryAfterPolicy;
use qubit_retry::RetryDelay;
use qubit_retry::RetryOptions;

/// Verifies the standalone builder creates a complete validated snapshot.
#[test]
fn test_retry_options_builder_builds_complete_snapshot() {
    let options = RetryOptions::builder()
        .max_retries(3)
        .max_operation_elapsed(Some(Duration::from_secs(4)))
        .max_total_elapsed(Some(Duration::from_secs(5)))
        .fixed_delay(Duration::from_millis(20))
        .jitter_factor(0.25)
        .attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
        .attempt_timeout(Some(Duration::from_millis(100)))
        .worker_cancel_grace(Duration::from_millis(30))
        .retry_after_policy(RetryAfterPolicy::AtLeastConfiguredDelay)
        .build()
        .expect("options should build");

    assert_eq!(4, options.max_attempts());
    assert_eq!(
        Some(Duration::from_secs(4)),
        options.max_operation_elapsed()
    );
    assert_eq!(Some(Duration::from_secs(5)), options.max_total_elapsed());
    assert_eq!(
        &RetryDelay::fixed(Duration::from_millis(20)),
        options.delay()
    );
    assert_eq!(
        Some(AttemptTimeoutPolicy::Abort),
        options.attempt_timeout().map(|option| option.policy())
    );
    assert_eq!(Duration::from_millis(30), options.worker_cancel_grace());
    assert_eq!(
        RetryAfterPolicy::AtLeastConfiguredDelay,
        options.retry_after_policy()
    );
}

/// Verifies the standalone builder rejects invalid option values.
#[test]
fn test_retry_options_builder_validates_options() {
    let error = RetryOptions::builder()
        .max_attempts(0)
        .build()
        .expect_err("zero attempts should be rejected");

    assert_eq!("max_attempts", error.path());
}
