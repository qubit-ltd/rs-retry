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
use qubit_retry::RetryContext;
use qubit_retry::RetryObserver;

#[test]
fn test_observer_trait_accepts_function_callbacks() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let observer: Box<dyn RetryObserver<()>> = Box::new(
        move |failure: &AttemptFailure<()>, context: &RetryContext| {
            assert_eq!(failure, &AttemptFailure::Error(()));
            assert_eq!(context.attempts(), 0);
            captured.fetch_add(1, Ordering::SeqCst);
        },
    );
    observer.on_attempt_failed(
        &AttemptFailure::Error(()),
        &RetryContext::new(0, 1),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
