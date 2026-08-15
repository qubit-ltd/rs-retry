// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public admission-plan completion behavior.

use qubit_retry::Retry;
use qubit_retry::RetryPolicy;

/// Verifies a completed admitted attempt clears its public attempt overlay.
#[test]
fn test_attempt_plan_success_clears_current_attempt_scope() {
    let policy = RetryPolicy::builder().build().unwrap();
    let result = Retry::<String>::builder(policy)
        .build()
        .sync()
        .run(|| Ok(42_u32))
        .expect("the first operation should succeed");
    assert_eq!(*result.value(), 42);
    assert_eq!(result.context().attempts(), 1);
    assert_eq!(result.context().current_attempt(), None);
    assert_eq!(result.context().current_attempt_timeout(), None);
}
