// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Retry facade behavior is covered through current API regressions.

use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryPolicy;

#[derive(Debug)]
struct NonCloneError;

#[test]
fn retry_clone_does_not_require_a_cloneable_error() {
    let retry = Retry::<io::Error>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("retry policy should build"),
    )
    .build();

    let cloned = retry.clone();
    assert_eq!(cloned.policy().limits().max_attempts().get(), 2);
}

#[test]
fn retry_clone_shares_callbacks_but_not_attempt_state() {
    let rules = Arc::new(AtomicUsize::new(0));
    let observers = Arc::new(AtomicUsize::new(0));
    let rule_counter = Arc::clone(&rules);
    let observer_counter = Arc::clone(&observers);
    let original = Retry::<NonCloneError>::builder(RetryPolicy::builder().max_attempts(2).build().expect("policy"))
        .rule(move |_: &AttemptFailure<NonCloneError>, _: &RetryContext| {
            rule_counter.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .observer(move |_: &AttemptFailure<NonCloneError>, _: &RetryContext| {
            observer_counter.fetch_add(1, Ordering::SeqCst);
        })
        .build();
    let copied = original.clone();

    for retry in [&original, &copied] {
        let mut calls = 0;
        let success = retry
            .sync()
            .run(|| {
                calls += 1;
                if calls == 1 { Err(NonCloneError) } else { Ok(42) }
            })
            .expect("second attempt succeeds");
        assert_eq!(success.context().attempts(), 2);
        assert_eq!(*success.value(), 42);
    }

    assert_eq!(rules.load(Ordering::SeqCst), 2);
    assert_eq!(observers.load(Ordering::SeqCst), 2);
}
