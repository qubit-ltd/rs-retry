// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryPolicy;

#[test]
fn test_policy_exposes_retry_limits() {
    let policy = RetryPolicy::builder().max_attempts(4).build().unwrap();
    assert_eq!(policy.admission_limits().max_attempts().get(), 4);
}
