// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;

use qubit_retry::Retry;
use qubit_retry::RetryContext;

use crate::support::TestError;

/// Verifies each attempt is announced before execution and successful
/// completion reports the committed attempt number.
#[test]
fn test_attempt_lifecycle_orders_before_attempt_operation_and_success() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let before_events = Arc::clone(&events);
    let success_events = Arc::clone(&events);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .before_attempt(move |context: &RetryContext| {
            before_events
                .lock()
                .expect("lifecycle events should be lockable")
                .push(format!("before:{}", context.attempt()));
        })
        .on_success(move |context: &RetryContext| {
            success_events
                .lock()
                .expect("lifecycle events should be lockable")
                .push(format!("success:{}", context.attempt()));
        })
        .build()
        .expect("retry should build");
    let operation_events = Arc::clone(&events);
    let mut attempts = 0;

    let value = retry
        .run(|| {
            attempts += 1;
            operation_events
                .lock()
                .expect("lifecycle events should be lockable")
                .push(format!("operation:{attempts}"));
            if attempts == 1 {
                Err(TestError("retry"))
            } else {
                Ok("done")
            }
        })
        .expect("the second attempt should succeed");

    assert_eq!(value.into_value(), "done");
    assert_eq!(
        *events.lock().expect("lifecycle events should be lockable"),
        vec![
            "before:1",
            "operation:1",
            "before:2",
            "operation:2",
            "success:2",
        ],
    );
}
