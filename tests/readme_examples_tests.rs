// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

/// Executes the README's bounded synchronous cancellation example.
#[test]
fn test_readme_synchronous_cancellation() {
    use qubit_retry::{Retry, RetryCancellationToken, RetryFailure, RetryPolicy};

    let token = RetryCancellationToken::new();
    let policy = RetryPolicy::builder().build().expect("valid policy");
    let retry = Retry::<&'static str>::builder(policy).build();
    let mut calls = 0;
    let error = retry.sync().cancellation_token(token.clone()).run(|| {
        calls += 1;
        token.cancel();
        Err::<(), _>("temporarily unavailable")
    }).expect_err("cancellation stops further attempts");
    assert_eq!(calls, 1);
    assert!(matches!(error.failure(), RetryFailure::Cancelled { .. }));
}

/// Executes the README's completion diagnostic and consuming error map example.
#[test]
fn test_readme_completion_diagnostics_and_error_mapping() {
    use qubit_retry::{Retry, RetryCallbackPhase, RetryContext, RetryFailure};
    use qubit_retry::{RetryObserver, RetryPolicy};

    struct CompletionAudit;
    impl RetryObserver<&'static str> for CompletionAudit {
        fn on_terminal_failure(&self, _: &RetryFailure<&'static str>, _: &RetryContext) {
            panic!("audit sink unavailable");
        }
    }

    let policy = RetryPolicy::builder().build().expect("valid policy");
    let retry = Retry::<&'static str>::builder(policy)
        .observer(CompletionAudit)
        .build();
    let error = retry.sync().run(|| Err::<(), _>("offline")).unwrap_err();
    let mapped = error.map_error(String::from);
    assert_eq!(mapped.last_error().map(String::as_str), Some("offline"));
    assert_eq!(mapped.completion_callback_failures().len(), 1);
    assert_eq!(mapped.completion_callback_failures()[0].phase(), RetryCallbackPhase::TerminalFailure);
    let (_failure, context, diagnostics) = mapped.into_parts_with_diagnostics();
    assert_eq!(context.attempts(), 3);
    assert_eq!(diagnostics.len(), 1);
}
