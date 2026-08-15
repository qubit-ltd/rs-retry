// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailure;
use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryTimeoutScope;

/// Verifies external callers can inspect every terminal classification and use
/// the common accessors without constructing non-exhaustive variants.
#[test]
fn test_retry_failure_external_shape_and_accessor_signatures() {
    /// Type-checks the externally visible terminal failure shape.
    fn inspect<E: std::fmt::Display>(failure: &RetryFailure<E>) {
        let _: Option<&AttemptFailure<E>> = failure.last_failure();
        let _: Option<&E> = failure.last_error();
        let _: String = failure.to_string();
        match failure {
            RetryFailure::Aborted { last_failure, .. } => {
                let _: &AttemptFailure<E> = last_failure;
            }
            RetryFailure::Exhausted {
                limit,
                last_failure,
                ..
            } => {
                let _: &RetryLimitKind = limit;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::TimedOut {
                scope,
                last_failure,
                ..
            } => {
                let _: &RetryTimeoutScope = scope;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::Cancelled {
                phase,
                last_failure,
                ..
            } => {
                let _: &RetryCancellationPhase = phase;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::CallbackFailed {
                callback,
                last_failure,
                ..
            } => {
                let _: &RetryCallbackFailure = callback;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::Infrastructure {
                failure,
                last_failure,
                ..
            } => {
                let _: &RetryInfrastructureFailure = failure;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            _ => {}
        }
    }

    let _: fn(&RetryFailure<String>) = inspect::<String>;
}
