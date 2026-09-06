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
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
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
                limit, last_failure, ..
            } => {
                let _: &RetryLimitKind = limit;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::TimedOut {
                scope, last_failure, ..
            } => {
                let _: &RetryTimeoutScope = scope;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::Cancelled {
                phase, last_failure, ..
            } => {
                let _: &RetryCancellationPhase = phase;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::CallbackFailed {
                callback, last_failure, ..
            } => {
                let _: &RetryCallbackFailure = callback;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            RetryFailure::Infrastructure {
                failure, last_failure, ..
            } => {
                let _: &RetryInfrastructureFailure = failure;
                let _: &Option<AttemptFailure<E>> = last_failure;
            }
            _ => {}
        }
    }

    let _: fn(&RetryFailure<String>) = inspect::<String>;
}

/// Observer that fails during the pre-admission notification.
struct StartedPanickingObserver;

impl RetryObserver<String> for StartedPanickingObserver {
    /// Panics before the attempt is admitted.
    fn on_attempt_started(&self, _context: &RetryContext) {
        panic!("started observer panic");
    }
}

/// Verifies mapping preserves an abort carrying a worker panic.
#[test]
fn map_error_preserves_aborted_failure_fields() {
    let failure =
        Retry::<String>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .worker()
            .run(|_: AttemptCancellationToken| -> Result<(), String> {
                panic!("operation panic");
            })
            .expect_err("worker panic must abort the flow")
            .into_failure();

    let mapped: RetryFailure<usize> =
        failure.map_error(|_| panic!("panic failure must not call the mapper"));

    let RetryFailure::Aborted { last_failure, .. } = mapped else {
        panic!("expected an aborted failure");
    };
    assert_eq!(
        last_failure,
        AttemptFailure::Panicked {
            panic: RetryPanic::StaticStr("operation panic"),
        }
    );
}

/// Verifies mapping preserves an exhausted limit and transforms its error.
#[test]
fn map_error_preserves_exhausted_failure_fields() {
    let failure = Retry::<String>::builder(
        RetryPolicy::builder().max_attempts(1).build().unwrap(),
    )
    .build()
    .sync()
    .run(|| Err::<(), _>(String::from("exhausted")))
    .expect_err("one failed attempt must exhaust the flow")
    .into_failure();

    let mapped = failure.map_error(String::into_bytes);

    let RetryFailure::Exhausted {
        limit,
        last_failure,
        ..
    } = mapped
    else {
        panic!("expected an exhausted failure");
    };
    assert_eq!(limit, RetryLimitKind::Attempts);
    assert_eq!(
        last_failure,
        Some(AttemptFailure::Error(b"exhausted".to_vec()))
    );
}

/// Verifies mapping preserves timeout scope and an absent last failure.
#[test]
fn map_error_preserves_timed_out_failure_fields() {
    let failure =
        Retry::<String>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .worker()
            .attempt_timeout(Duration::ZERO)
            .run(|_: AttemptCancellationToken| Ok::<(), String>(()))
            .expect_err("a zero attempt timeout must stop before admission")
            .into_failure();

    let mapped: RetryFailure<usize> =
        failure.map_error(|_| panic!("empty timeout must not call the mapper"));

    let RetryFailure::TimedOut {
        scope,
        last_failure,
        ..
    } = mapped
    else {
        panic!("expected a timed out failure");
    };
    assert_eq!(scope, RetryTimeoutScope::Attempt);
    assert_eq!(last_failure, None);
}

/// Verifies mapping preserves cancellation phase and an absent last failure.
#[test]
fn map_error_preserves_cancelled_failure_fields() {
    let cancellation = RetryCancellationToken::new();
    cancellation.cancel();
    let failure =
        Retry::<String>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .sync()
            .cancellation_token(cancellation)
            .run(|| Ok::<(), String>(()))
            .expect_err("a pre-cancelled flow must stop before admission")
            .into_failure();

    let mapped: RetryFailure<usize> = failure
        .map_error(|_| panic!("empty cancellation must not call the mapper"));

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = mapped
    else {
        panic!("expected a cancelled failure");
    };
    assert_eq!(phase, RetryCancellationPhase::BeforeAttempt);
    assert_eq!(last_failure, None);
}

/// Verifies mapping preserves callback attribution and an absent last failure.
#[test]
fn map_error_preserves_callback_failed_fields() {
    let failure =
        Retry::<String>::builder(RetryPolicy::builder().build().unwrap())
            .observer(StartedPanickingObserver)
            .build()
            .sync()
            .run(|| Ok::<(), String>(()))
            .expect_err("the pre-admission observer must fail closed")
            .into_failure();

    let mapped: RetryFailure<usize> = failure.map_error(|_| {
        panic!("empty callback failure must not call the mapper")
    });

    let RetryFailure::CallbackFailed {
        callback,
        last_failure,
        ..
    } = mapped
    else {
        panic!("expected a callback failure");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 0);
    assert_eq!(callback.phase(), RetryCallbackPhase::AttemptStarted);
    assert_eq!(callback.panic().message(), Some("started observer panic"));
    assert_eq!(last_failure, None);
}

/// Verifies mapping preserves infrastructure data and an absent last failure.
#[test]
fn map_error_preserves_infrastructure_failure_fields() {
    let failure =
        Retry::<String>::builder(RetryPolicy::builder().build().unwrap())
            .build()
            .worker()
            .worker_stack_size(usize::MAX)
            .run(|_: AttemptCancellationToken| Ok::<(), String>(()))
            .expect_err("an impossible stack size must fail worker creation")
            .into_failure();

    let mapped: RetryFailure<usize> = failure.map_error(|_| {
        panic!("empty infrastructure failure must not call the mapper")
    });

    let RetryFailure::Infrastructure {
        failure,
        last_failure,
        ..
    } = mapped
    else {
        panic!("expected an infrastructure failure");
    };
    let RetryInfrastructureFailure::WorkerSpawn { message } = failure else {
        panic!("expected worker-spawn attribution");
    };
    assert!(!message.is_empty());
    assert_eq!(last_failure, None);
}
