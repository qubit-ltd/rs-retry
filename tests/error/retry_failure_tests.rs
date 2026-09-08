// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::AttemptCancellationToken;
use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackFailure;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;

#[test]
fn test_retry_error_reason_shape_and_accessors_are_separate() {
    fn inspect(failure: &RetryErrorReason) {
        let _: String = failure.to_string();
        match failure {
            RetryErrorReason::Aborted => {}
            RetryErrorReason::Exhausted { limit } => {
                let _: &RetryLimitKind = limit;
            }
            RetryErrorReason::TimedOut { scope } => {
                let _: &RetryTimeoutScope = scope;
            }
            RetryErrorReason::Cancelled { phase } => {
                let _: &RetryCancellationPhase = phase;
            }
            RetryErrorReason::CallbackFailed { callback } => {
                let _: &RetryCallbackFailure = callback;
            }
            RetryErrorReason::Infrastructure { failure } => {
                let _: &RetryInfrastructureFailure = failure;
            }
            _ => {}
        }
    }
    let _: fn(&RetryErrorReason) = inspect;
}

struct StartedPanickingObserver;

impl RetryObserver<String> for StartedPanickingObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        panic!("started observer panic");
    }
}

#[test]
fn test_map_error_preserves_aborted_worker_panic() {
    let error = Retry::<String>::builder(RetryPolicy::builder().build().expect("policy"))
        .build()
        .worker()
        .run(|_: AttemptCancellationToken| -> Result<(), String> {
            panic!("operation panic");
        })
        .expect_err("worker panic must abort the flow");
    assert!(matches!(error.reason(), RetryErrorReason::Aborted));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Panicked {
            panic: RetryPanic::StaticStr("operation panic")
        })
    ));
    let mapped = error.map_error(|value| value.into_bytes());
    assert!(matches!(mapped.reason(), RetryErrorReason::Aborted));
    assert!(matches!(mapped.last_failure(), Some(AttemptFailure::Panicked { .. })));
}

#[test]
fn test_map_error_preserves_exhausted_application_failure() {
    let error = Retry::<String>::builder(RetryPolicy::builder().max_attempts(1).build().expect("policy"))
        .fallback(RetryFallback::Retry)
        .build()
        .sync()
        .run(|| Err::<(), _>(String::from("exhausted")))
        .expect_err("one failed attempt must exhaust the flow");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Exhausted {
            limit: RetryLimitKind::Attempts
        }
    ));
    let mapped = error.map_error(String::into_bytes);
    assert_eq!(mapped.last_error(), Some(&b"exhausted".to_vec()));
}

#[test]
fn test_retry_error_retains_timeout_and_cancellation_failures() {
    let timeout = Retry::<String>::builder(RetryPolicy::builder().build().expect("policy"))
        .build()
        .worker()
        .hard_attempt_timeout(Duration::ZERO)
        .run(|_: AttemptCancellationToken| Ok::<(), String>(()))
        .expect_err("zero attempt timeout must stop before admission");
    assert!(matches!(
        timeout.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Attempt
        }
    ));
    assert!(timeout.last_failure().is_none());

    let token = RetryCancellationToken::new();
    token.cancel();
    let cancelled = Retry::<String>::builder(RetryPolicy::builder().build().expect("policy"))
        .build()
        .sync()
        .cancellation_token(token)
        .run(|| Ok::<(), String>(()))
        .expect_err("pre-cancelled flow must stop before admission");
    assert!(matches!(
        cancelled.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::BeforeAttempt
        }
    ));
    assert!(cancelled.last_failure().is_none());
}

#[test]
fn test_retry_error_retains_callback_and_infrastructure_classification() {
    let callback = Retry::<String>::builder(RetryPolicy::builder().build().expect("policy"))
        .observer(StartedPanickingObserver)
        .build()
        .sync()
        .run(|| Ok::<(), String>(()))
        .expect_err("observer panic must fail closed");
    assert!(matches!(
        callback.reason(),
        RetryErrorReason::CallbackFailed { callback }
            if callback.callback() == RetryCallbackKind::Observer
                && callback.phase() == RetryCallbackPhase::BeforeAttempt
    ));
    assert!(callback.last_failure().is_none());

    let infrastructure = Retry::<String>::builder(RetryPolicy::builder().build().expect("policy"))
        .build()
        .worker()
        .worker_stack_size(usize::MAX)
        .run(|_: AttemptCancellationToken| Ok::<(), String>(()))
        .expect_err("impossible stack size must fail worker creation");
    assert!(matches!(
        infrastructure.reason(),
        RetryErrorReason::Infrastructure {
            failure: RetryInfrastructureFailure::WorkerSpawn { .. }
        }
    ));
    assert!(infrastructure.last_failure().is_none());
}
