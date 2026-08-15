// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::panic::panic_any;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;

struct NoopRule;

impl RetryRule<()> for NoopRule {
    fn decide(
        &self,
        _failure: &AttemptFailure<()>,
        _context: &RetryContext,
    ) -> RetryDecision {
        RetryDecision::UseDefault
    }
}

#[test]
fn test_retry_builder_accepts_ordered_rules() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).rule(NoopRule).build();
    let _ = retry;
}

/// Verifies every rule panic payload reaches the public callback terminal.
#[test]
fn test_retry_rules_preserve_each_panic_payload() {
    let cases = [
        (0_u8, RetryPanic::StaticStr("static panic")),
        (1_u8, RetryPanic::String(String::from("owned panic"))),
        (2_u8, RetryPanic::NonString),
    ];
    for (payload, expected) in cases {
        let later_calls = Arc::new(AtomicUsize::new(0));
        let retry =
            Retry::<()>::builder(RetryPolicy::builder().build().unwrap())
                .rule(move |_: &AttemptFailure<()>, _: &RetryContext| {
                    match payload {
                        0 => panic!("static panic"),
                        1 => panic_any(String::from("owned panic")),
                        _ => panic_any(17_u32),
                    }
                })
                .rule({
                    let later_calls = Arc::clone(&later_calls);
                    move |_: &AttemptFailure<()>, _: &RetryContext| {
                        later_calls.fetch_add(1, Ordering::SeqCst);
                        RetryDecision::UseDefault
                    }
                })
                .build();
        let error = retry
            .sync()
            .run(|| Err::<(), _>(()))
            .expect_err("the rule must panic");
        let RetryFailure::CallbackFailed { callback, .. } = error.failure()
        else {
            panic!("expected a callback-failure terminal");
        };
        assert_eq!(callback.callback(), RetryCallbackKind::Rule);
        assert_eq!(callback.phase(), RetryCallbackPhase::RuleDecision);
        assert_eq!(callback.panic(), &expected);
        assert_eq!(later_calls.load(Ordering::SeqCst), 0);
    }
}
