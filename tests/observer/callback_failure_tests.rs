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
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

struct PanickingObserver;

impl RetryObserver<TestError> for PanickingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        panic!("observer panic");
    }
}

/// Verifies an earlier observer panic prevents the later rule callback.
#[test]
fn test_callback_failure_stops_later_callback_kinds() {
    let rule_calls = Arc::new(AtomicUsize::new(0));
    let retry =
        Retry::<TestError>::builder(RetryPolicy::builder().build().unwrap())
            .observer(PanickingObserver)
            .rule({
                let rule_calls = Arc::clone(&rule_calls);
                move |_: &AttemptFailure<TestError>, _: &RetryContext| {
                    rule_calls.fetch_add(1, Ordering::SeqCst);
                    RetryDecision::UseDefault
                }
            })
            .build();
    let error = retry
        .sync()
        .run(|| Err::<(), _>(TestError("retry")))
        .expect_err("the observer must panic first");
    let RetryFailure::CallbackFailed { callback, .. } = error.failure() else {
        panic!("expected a callback-failure terminal");
    };
    assert_eq!(callback.callback(), RetryCallbackKind::Observer);
    assert_eq!(callback.index(), 0);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
}
