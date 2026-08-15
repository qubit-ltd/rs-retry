// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-directive scheduling behavior.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

type RetryHintRecord = Option<(Option<Duration>, Option<Duration>)>;

struct HintRecordingObserver(Arc<Mutex<RetryHintRecord>>);

impl RetryObserver<TestError> for HintRecordingObserver {
    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        context: &RetryContext,
    ) {
        *self.0.lock().unwrap() =
            Some((context.retry_after_hint(), context.next_delay()));
    }
}

/// Verifies a retry directive exposes its hint and resolved schedule overlay.
#[test]
fn test_retry_directive_records_retry_hint_and_resolved_delay() {
    let recorded = Arc::new(Mutex::new(None));
    let attempts = AtomicU32::new(0);
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::ZERO).ignore_retry_after())
        .build()
        .unwrap();
    let result = Retry::<TestError>::builder(policy)
        .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithHint(Duration::from_secs(3))
        })
        .observer(HintRecordingObserver(Arc::clone(&recorded)))
        .build()
        .sync()
        .run(|| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TestError("retry"))
            } else {
                Ok(())
            }
        })
        .expect("hinted retry should succeed");
    assert_eq!(
        *recorded.lock().unwrap(),
        Some((Some(Duration::from_secs(3)), Some(Duration::ZERO))),
    );
    assert_eq!(result.context().retry_after_hint(), None);
}
