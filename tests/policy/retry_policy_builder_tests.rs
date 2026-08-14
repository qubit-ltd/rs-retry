// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;

#[test]
fn builder_accepts_backoff_policy() {
    let delay = std::time::Duration::from_millis(1);
    let policy = RetryPolicy::builder()
        .backoff(BackoffPolicy::fixed(delay))
        .build()
        .unwrap();
    assert_eq!(policy.backoff().maximum_delay(), Some(delay));
}
