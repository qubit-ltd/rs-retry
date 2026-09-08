// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;
/// Executes the guide's synchronous cancellation contract.
#[test]
fn test_readme_synchronous_cancellation() {
    let token = RetryCancellationToken::new();
    let policy = RetryPolicy::builder().build().expect("valid policy");
    let retry = Retry::<&'static str>::builder(policy).build();
    let mut calls = 0;
    let error = retry
        .sync()
        .cancellation_token(token.clone())
        .run(|| {
            calls += 1;
            token.cancel();
            Err::<(), _>("temporarily unavailable")
        })
        .expect_err("cancellation stops further attempts");
    assert_eq!(calls, 1);
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled { .. }));
}

/// Executes the guide's completion diagnostic and consuming error map contract.
#[test]
fn test_readme_completion_diagnostics_and_error_mapping() {
    struct CompletionAudit;
    impl RetryObserver<&'static str> for CompletionAudit {
        fn on_terminal_failure(&self, _: &RetryErrorReason, _: &RetryContext) {
            panic!("audit sink unavailable");
        }
    }

    let policy = RetryPolicy::builder().build().expect("valid policy");
    let retry = Retry::<&'static str>::builder(policy).observer(CompletionAudit).build();
    let error = retry.sync().run(|| Err::<(), _>("offline")).unwrap_err();
    let mapped = error.map_error(String::from);
    assert_eq!(mapped.last_error().map(String::as_str), Some("offline"));
    assert_eq!(mapped.completion_callback_failures().len(), 1);
    assert_eq!(
        mapped.completion_callback_failures()[0].phase(),
        RetryCallbackPhase::TerminalFailure
    );
    let (_reason, _failure, context, diagnostics) = mapped.into_parts();
    assert_eq!(context.attempts(), 3);
    assert_eq!(diagnostics.len(), 1);
}
