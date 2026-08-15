// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-limit behavior.

use qubit_retry::RetryLimitKind;

#[test]
fn test_retry_limit_kind_display() {
    let cases = [
        (RetryLimitKind::Attempts, "attempts"),
        (RetryLimitKind::OperationElapsed, "operation elapsed"),
        (RetryLimitKind::TotalElapsed, "total elapsed"),
    ];
    for (kind, expected) in cases {
        assert_eq!(kind.to_string(), expected);
    }
}
