// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Public attempt-failure mapping behavior.

use qubit_retry::AttemptFailure;
use qubit_retry::RetryPanic;
use qubit_retry::RetryTimeoutScope;

/// Application error that deliberately does not implement `Clone`.
struct NonCloneError(String);

/// Verifies mapping consumes a non-clone application error with an `FnOnce`.
#[test]
fn test_map_error_consumes_application_error_once() {
    let suffix = String::from("!");
    let original = AttemptFailure::Error(NonCloneError(String::from("retry")));

    let mapped = original.map_error(move |NonCloneError(mut error)| {
        error.push_str(&suffix);
        error
    });

    assert_eq!(mapped.into_error().as_deref(), Some("retry!"));
}

/// Verifies timeout metadata is retained without evaluating the mapper.
#[test]
fn test_map_error_does_not_call_mapper_for_timeout() {
    let failure = AttemptFailure::<String>::TimedOut {
        scope: RetryTimeoutScope::Attempt,
    };

    let mapped: AttemptFailure<usize> =
        failure.map_error(|_| panic!("timeout must not call the mapper"));

    assert_eq!(
        mapped,
        AttemptFailure::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        }
    );
}

/// Verifies panic metadata is retained without evaluating the mapper.
#[test]
fn test_map_error_does_not_call_mapper_for_panic() {
    let failure = AttemptFailure::<String>::Panicked {
        panic: RetryPanic::String(String::from("operation panic")),
    };

    let mapped: AttemptFailure<usize> =
        failure.map_error(|_| panic!("panic failure must not call the mapper"));

    assert_eq!(
        mapped,
        AttemptFailure::Panicked {
            panic: RetryPanic::String(String::from("operation panic")),
        }
    );
}
