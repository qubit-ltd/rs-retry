// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;

struct AbortRule;

impl RetryRule<()> for AbortRule {
    fn decide(&self, _: &AttemptFailure<()>, _: &RetryContext) -> RetryDecision {
        RetryDecision::Abort
    }
}

struct CountingRule(Arc<AtomicUsize>);

impl RetryRule<()> for CountingRule {
    fn decide(&self, _: &AttemptFailure<()>, _: &RetryContext) -> RetryDecision {
        self.0.fetch_add(1, Ordering::SeqCst);
        RetryDecision::Abort
    }
}

#[test]
fn unclassified_application_error_aborts_by_default() {
    let retry = Retry::<&str>::builder(RetryPolicy::builder().max_attempts(3).build().unwrap()).build();
    let calls = AtomicUsize::new(0);
    let error = retry
        .sync()
        .run(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("permanent")
        })
        .unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(error.reason(), RetryErrorReason::Aborted));
}

#[test]
fn explicit_retry_fallback_retries_unclassified_errors() {
    let retry = Retry::<&str>::builder(RetryPolicy::builder().max_attempts(2).build().unwrap())
        .fallback(RetryFallback::Retry)
        .build();
    let calls = AtomicUsize::new(0);
    let error = retry
        .sync()
        .run(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("temporary")
        })
        .unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(matches!(error.reason(), RetryErrorReason::Exhausted { .. }));
}

#[test]
fn explicit_rule_keeps_order_and_reason() {
    let retry = Retry::<()>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .rule(AbortRule)
        .build();
    let error = retry.sync().run(|| Err::<(), _>(())).unwrap_err();
    assert!(matches!(error.reason(), RetryErrorReason::Aborted));
    assert_eq!(error.last_error(), Some(&()));
}

#[test]
fn clone_shares_callbacks_and_preserves_behavior() {
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let retry = Retry::<()>::builder(RetryPolicy::builder().max_attempts(2).build().unwrap())
        .rule(CountingRule(callback_calls))
        .build();
    let cloned = retry.clone();
    let _ = cloned.sync().run(|| Err::<(), _>(()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn metadata_split_moves_only_application_error() {
    let retry = Retry::<String>::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .fallback(RetryFallback::Abort)
        .build();
    let error = retry.sync().run(|| Err::<(), _>("offline".to_owned())).unwrap_err();
    let (metadata, application_error) = error.into_metadata_and_error();
    assert!(matches!(metadata.reason(), RetryErrorReason::Aborted));
    assert_eq!(application_error.as_deref(), Some("offline"));
}
