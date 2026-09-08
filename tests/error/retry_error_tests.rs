// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Terminal retry errors are covered through public executor outcomes.

use std::error::Error;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryLimitKind;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

/// Verifies the public retry error view is lossless and exposes the last
/// application error as its standard error source.
#[test]
fn test_retry_error_preserves_terminal_failure_and_context() {
    let retry = RetryConfig::<TestError>::builder().max_attempts(2)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| RetryDecision::Abort)
        .build().expect("valid config");

    let error = Retry::new(&retry)
        .run(|| Err::<(), _>(TestError("fatal")))
        .expect_err("the rule should abort the retry flow");

    assert!(matches!(error.reason(), RetryErrorReason::Aborted));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("fatal")));
    assert_eq!(Error::source(&error).map(ToString::to_string), Some("fatal".to_owned()));
    assert_eq!(error.to_string(), "retry aborted after 1 attempt(s)");

    let (reason, last_failure, context, diagnostics) = error.into_parts();
    assert!(diagnostics.is_empty());
    assert!(matches!(reason, RetryErrorReason::Aborted));
    assert!(matches!(last_failure, Some(AttemptFailure::Error(TestError("fatal")))));
    assert_eq!(context.attempts(), 1);
}

/// Application error that deliberately does not implement `Clone`.
#[derive(Debug, PartialEq, Eq)]
struct NonCloneError(String);

/// Observer that adds a completion diagnostic to a terminal result.
struct TerminalPanickingObserver;

impl RetryObserver<NonCloneError> for TerminalPanickingObserver {
    /// Panics during terminal completion notification.
    fn on_terminal_failure(&self, _failure: &RetryErrorReason, _context: &RetryContext) {
        panic!("terminal observer panic");
    }
}

/// Verifies retry-error mapping preserves context and completion diagnostics.
#[test]
fn test_map_error_preserves_context_and_completion_diagnostics() {
    let config = RetryConfig::<NonCloneError>::builder().max_attempts(1)
        .observer(TerminalPanickingObserver)
        .fallback(RetryFallback::Retry)
        .build().expect("valid config");
    let error = Retry::new(&config)
        .run(|| Err::<(), _>(NonCloneError(String::from("retry"))))
        .expect_err("the only failed attempt must exhaust the flow");
    let expected_context = *error.context();
    let suffix = String::from("!");

    let mapped = error.map_error(move |NonCloneError(mut error)| {
        error.push_str(&suffix);
        error
    });

    assert_eq!(*mapped.context(), expected_context);
    let RetryErrorReason::Exhausted { limit } = mapped.reason() else {
        panic!("expected an exhausted failure");
    };
    assert_eq!(*limit, RetryLimitKind::Attempts);
    assert_eq!(mapped.last_error(), Some(&String::from("retry!")));
    let [diagnostic] = mapped.completion_callback_failures() else {
        panic!("expected one completion diagnostic");
    };
    assert_eq!(diagnostic.callback(), RetryCallbackKind::Observer);
    assert_eq!(diagnostic.index(), 0);
    assert_eq!(diagnostic.phase(), RetryCallbackPhase::TerminalFailure);
    assert_eq!(diagnostic.panic().message(), Some("terminal observer panic"));
}
