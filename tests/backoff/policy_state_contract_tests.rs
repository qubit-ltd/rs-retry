// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::RetryPolicy;

#[test]
fn test_policy_validates_limits_and_backoff_state() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
    let policy = BackoffPolicy::exponential(Duration::from_millis(10), 2.0, Duration::from_millis(25)).unwrap();
    let mut state = policy.start();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(20)
    );
    state.reset();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
}
