// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public terminal behavior backed by callback-panic conversion.

use std::panic::panic_any;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
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

    /// Returns the stable panic representation expected at the public API.
    fn expected(self) -> RetryPanic {
        match self {
            Self::StaticStr => RetryPanic::StaticStr("static panic"),
            Self::String => RetryPanic::String(String::from("owned panic")),
            Self::NonString => RetryPanic::NonString,
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
    fn on_before_attempt(&self, _context: &RetryContext) {
        if self.phase == RetryCallbackPhase::BeforeAttempt {
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

struct CountedPayload(Arc<AtomicUsize>);

impl Drop for CountedPayload {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl RetryObserver<TestError> for CountingObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        if self.phase == RetryCallbackPhase::BeforeAttempt {
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

/// Verifies every observer phase and payload reaches the public terminal.
#[test]
fn test_retry_panic_from_payload_stops_later_callbacks_for_each_case() {
    let cases = [
        (RetryCallbackPhase::BeforeAttempt, PanicPayload::StaticStr),
        (RetryCallbackPhase::BeforeAttempt, PanicPayload::String),
        (RetryCallbackPhase::BeforeAttempt, PanicPayload::NonString),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::StaticStr),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::String),
        (RetryCallbackPhase::AttemptFailed, PanicPayload::NonString),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::StaticStr),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::String),
        (RetryCallbackPhase::RetryScheduled, PanicPayload::NonString),
    ];
    for (phase, payload) in cases {
        let later_calls = Arc::new(AtomicUsize::new(0));
        let retry = RetryConfig::<TestError>::builder()
            .policy(two_attempt_policy())
            .observer(NoopObserver)
            .observer(PanickingObserver { phase, payload })
            .observer(CountingObserver {
                phase,
                calls: Arc::clone(&later_calls),
            })
            .fallback(RetryFallback::Retry)
            .build()
            .expect("valid config");
        let error = Retry::new(&retry)
            .run(|| Err::<(), _>(TestError("retry")))
            .expect_err("the selected callback should panic");
        let RetryErrorReason::CallbackFailed { callback, .. } = error.reason()
        else {
            panic!("expected a public callback-failure terminal");
        };
        assert_eq!(callback.callback(), RetryCallbackKind::Observer);
        assert_eq!(callback.index(), 1);
        assert_eq!(callback.phase(), phase);
        assert_eq!(callback.panic(), &payload.expected());
        assert_eq!(
            later_calls.load(Ordering::SeqCst),
            0,
            "later callback ran after panic in {phase:?}"
        );
    }
}

#[test]
fn test_control_payload_normal_drop_is_not_leaked() {
    let drops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&drops);
    let retry = RetryConfig::<&'static str>::builder()
        .rule(move |_: &AttemptFailure<&'static str>, _: &RetryContext| {
            panic_any(CountedPayload(Arc::clone(&captured)))
        })
        .build()
        .expect("valid config");

    let error = Retry::new(&retry)
        .run(|| Err::<(), _>("business"))
        .expect_err("rule panic should terminate the retry");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::CallbackFailed { .. }
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
