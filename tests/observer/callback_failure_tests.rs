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
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

use crate::support::TestError;

#[derive(Clone, Copy)]
enum PanicPayload {
    StaticStr,
    String,
    NonString,
}

impl PanicPayload {
    /// Panics with the payload represented by this test case.
    fn raise(self) -> ! {
        match self {
            Self::StaticStr => panic!("static panic"),
            Self::String => panic_any(String::from("owned panic")),
            Self::NonString => panic_any(17_u32),
        }
    }
}

struct PanickingObserver {
    phase: RetryCallbackPhase,
    payload: PanicPayload,
}

struct NoopObserver;

impl RetryObserver<TestError> for NoopObserver {}

impl RetryObserver<TestError> for PanickingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        if self.phase == RetryCallbackPhase::AttemptStarted {
            self.payload.raise();
        }
    }

    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::AttemptFailed {
            self.payload.raise();
        }
    }

    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::RetryScheduled {
            self.payload.raise();
        }
    }
}

struct CountingObserver {
    phase: RetryCallbackPhase,
    calls: Arc<AtomicUsize>,
}

impl RetryObserver<TestError> for CountingObserver {
    fn on_attempt_started(&self, _context: &RetryContext) {
        if self.phase == RetryCallbackPhase::AttemptStarted {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::AttemptFailed {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn on_retry_scheduled(
        &self,
        _backoff: &BackoffStep,
        _context: &RetryContext,
    ) {
        if self.phase == RetryCallbackPhase::RetryScheduled {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Builds a two-attempt policy so every observer phase is exercised.
fn two_attempt_policy() -> RetryPolicy {
    RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::fixed(Duration::ZERO))
        .build()
        .expect("the callback failure test policy should be valid")
}

/// Verifies that the first callback panic stops later callbacks in every phase.
#[test]
fn test_callbacks_first_panic_stops_later_callbacks_for_each_phase_and_payload()
{
    let cases = [
        (RetryCallbackPhase::AttemptStarted, PanicPayload::StaticStr),
        (RetryCallbackPhase::AttemptStarted, PanicPayload::String),
        (RetryCallbackPhase::AttemptStarted, PanicPayload::NonString),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::StaticStr),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::String),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::NonString),
        (RetryCallbackPhase::RuleDecision, PanicPayload::StaticStr),
        (RetryCallbackPhase::RuleDecision, PanicPayload::String),
        (RetryCallbackPhase::RuleDecision, PanicPayload::NonString),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::StaticStr),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::String),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::NonString),
    ];

    for (phase, payload) in cases {
        let later_calls = Arc::new(AtomicUsize::new(0));
        let retry = Retry::<TestError>::builder(two_attempt_policy())
            .observer(NoopObserver)
            .observer(PanickingObserver { phase, payload })
            .observer(CountingObserver {
                phase,
                calls: Arc::clone(&later_calls),
            })
            .rule(|_: &AttemptFailure<TestError>, _: &RetryContext| {
                RetryDecision::UseDefault
            })
            .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
                if phase == RetryCallbackPhase::RuleDecision {
                    payload.raise();
                }
                RetryDecision::UseDefault
            })
            .rule({
                let later_calls = Arc::clone(&later_calls);
                move |_: &AttemptFailure<TestError>, _: &RetryContext| {
                    if phase == RetryCallbackPhase::RuleDecision {
                        later_calls.fetch_add(1, Ordering::SeqCst);
                    }
                    RetryDecision::UseDefault
                }
            })
            .build();

        let _ = retry.sync().run(|| Err::<(), _>(TestError("retry")));

        assert_eq!(
            later_calls.load(Ordering::SeqCst),
            0,
            "later callback ran after panic in {phase:?}",
        );
    }
}
