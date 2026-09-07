// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailure;
use qubit_retry::RetryPanic;
use qubit_retry::RetryTimeoutScope;

use crate::support::TestError;

#[test]
fn test_error_model_exposes_all_terminal_parts() {
    let failures = [
        AttemptFailure::Error(TestError("application")),
        AttemptFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        },
        AttemptFailure::TimedOut {
            scope: RetryTimeoutScope::Flow,
        },
        AttemptFailure::Panicked {
            panic: RetryPanic::StaticStr("isolated"),
        },
    ];
    assert_eq!(failures[0].as_error(), Some(&TestError("application")));
    assert!(failures[1].is_timeout());
    assert_eq!(failures[1].timeout_scope(), Some(RetryTimeoutScope::Attempt));
    assert_eq!(failures[2].timeout_scope(), Some(RetryTimeoutScope::Flow));
    assert_eq!(failures[3].panic(), Some(&RetryPanic::StaticStr("isolated")));
    assert!(failures[3].as_error().is_none());
    assert!(failures[0].timeout_scope().is_none());
    assert!(failures[0].panic().is_none());
    assert!(failures[3].clone().into_error().is_none());
    assert_eq!(failures[0].clone().into_error(), Some(TestError("application")));
    assert_eq!(failures[0].to_string(), "application");
    assert_eq!(failures[1].to_string(), "attempt timed out (attempt)");
    assert_eq!(failures[3].to_string(), "attempt panicked: isolated");
}
