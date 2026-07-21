// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::{Arc, Mutex};

use qubit_retry::{AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryErrorReason};

use crate::support::TestError;

/// Verifies failure events run in registration order and the last concrete
/// listener decision wins.
#[test]
fn test_retry_events_resolve_all_failure_listener_decisions_in_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let first_events = Arc::clone(&events);
    let second_events = Arc::clone(&events);
    let third_events = Arc::clone(&events);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                first_events
                    .lock()
                    .expect("event order should be lockable")
                    .push(format!("first:{}", context.attempt()));
                AttemptFailureDecision::Abort
            },
        )
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                second_events
                    .lock()
                    .expect("event order should be lockable")
                    .push(format!("second:{}", context.attempt()));
                AttemptFailureDecision::UseDefault
            },
        )
        .on_failure(
            move |_failure: &AttemptFailure<TestError>, context: &RetryContext| {
                third_events
                    .lock()
                    .expect("event order should be lockable")
                    .push(format!("third:{}", context.attempt()));
                AttemptFailureDecision::Retry
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("the retry flow should exhaust both attempts");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(
        *events.lock().expect("event order should be lockable"),
        vec![
            "first:1", "second:1", "third:1", "first:2", "second:2", "third:2",
        ],
    );
}
