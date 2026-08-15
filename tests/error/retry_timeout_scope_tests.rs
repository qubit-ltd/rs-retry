// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public timeout-scope behavior.

use qubit_retry::RetryTimeoutScope;

#[test]
fn test_retry_timeout_scope_display() {
    let cases = [
        (RetryTimeoutScope::Attempt, "attempt"),
        (RetryTimeoutScope::Flow, "flow"),
    ];
    for (scope, expected) in cases {
        assert_eq!(scope.to_string(), expected);
    }
}
