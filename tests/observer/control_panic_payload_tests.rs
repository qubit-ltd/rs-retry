// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
// =============================================================================
//! Control callback payload destruction must not mask the callback terminal.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackKind;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryFailure;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPanic;
use qubit_retry::RetryPolicy;

struct DropPanicPayload {
    drops: Arc<AtomicUsize>,
    recursive: bool,
}

impl Drop for DropPanicPayload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        if self.recursive {
            std::panic::panic_any(Self {
                drops: Arc::clone(&self.drops),
                recursive: true,
            });
        }
        panic!("payload destructor panic");
    }
}

struct PayloadObserver {
    target: RetryCallbackPhase,
    drops: Arc<AtomicUsize>,
    recursive: bool,
}

impl PayloadObserver {
    fn raise_if(&self, phase: RetryCallbackPhase) {
        if self.target == phase {
            std::panic::panic_any(DropPanicPayload {
                drops: Arc::clone(&self.drops),
                recursive: self.recursive,
            });
        }
    }
}

impl RetryObserver<&'static str> for PayloadObserver {
    fn on_before_attempt(&self, _: &RetryContext) {
        self.raise_if(RetryCallbackPhase::BeforeAttempt);
    }

    fn on_attempt_failed(&self, _: &AttemptFailure<&'static str>, _: &RetryContext) {
        self.raise_if(RetryCallbackPhase::AttemptFailed);
    }

    fn on_retry_scheduled(&self, _: &BackoffStep, _: &RetryContext) {
        self.raise_if(RetryCallbackPhase::RetryScheduled);
    }
}

struct Noop;

impl RetryObserver<&'static str> for Noop {}

struct LaterObserver {
    target: RetryCallbackPhase,
    controls: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
}

impl LaterObserver {
    fn record(&self, phase: RetryCallbackPhase) {
        if self.target == phase {
            self.controls.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl RetryObserver<&'static str> for LaterObserver {
    fn on_before_attempt(&self, _: &RetryContext) {
        self.record(RetryCallbackPhase::BeforeAttempt);
    }

    fn on_attempt_failed(&self, _: &AttemptFailure<&'static str>, _: &RetryContext) {
        self.record(RetryCallbackPhase::AttemptFailed);
    }

    fn on_retry_scheduled(&self, _: &BackoffStep, _: &RetryContext) {
        self.record(RetryCallbackPhase::RetryScheduled);
    }

    fn on_terminal_failure(&self, _: &RetryFailure<&'static str>, _: &RetryContext) {
        self.completed.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
enum Facade {
    Sync,
    Worker,
    #[cfg(feature = "tokio")]
    Async,
}

async fn run_matrix(recursive: bool) {
    for facade in [
        Facade::Sync,
        Facade::Worker,
        #[cfg(feature = "tokio")]
        Facade::Async,
    ] {
        for phase in [
            RetryCallbackPhase::BeforeAttempt,
            RetryCallbackPhase::AttemptFailed,
            RetryCallbackPhase::RuleDecision,
            RetryCallbackPhase::RetryScheduled,
        ] {
            let drops = Arc::new(AtomicUsize::new(0));
            let later = Arc::new(AtomicUsize::new(0));
            let completed = Arc::new(AtomicUsize::new(0));
            let rule_drops = Arc::clone(&drops);
            let later_rule = Arc::clone(&later);
            let retry =
                Retry::<&'static str>::builder(RetryPolicy::builder().max_attempts(2).build().expect("valid policy"))
                    .observer(Noop)
                    .observer(PayloadObserver {
                        target: phase,
                        drops: Arc::clone(&drops),
                        recursive,
                    })
                    .observer(LaterObserver {
                        target: phase,
                        controls: Arc::clone(&later),
                        completed: Arc::clone(&completed),
                    })
                    .rule(|_: &AttemptFailure<&'static str>, _: &RetryContext| RetryDecision::UseDefault)
                    .rule(move |_: &AttemptFailure<&'static str>, _: &RetryContext| {
                        if phase == RetryCallbackPhase::RuleDecision {
                            std::panic::panic_any(DropPanicPayload {
                                drops: Arc::clone(&rule_drops),
                                recursive,
                            });
                        }
                        RetryDecision::Retry
                    })
                    .rule(move |_: &AttemptFailure<&'static str>, _: &RetryContext| {
                        later_rule.fetch_add(1, Ordering::SeqCst);
                        RetryDecision::Retry
                    })
                    .build();
            let result = match facade {
                Facade::Sync => retry.sync().run(|| Err::<(), _>("business")),
                Facade::Worker => retry.worker().run(|_| Err::<(), _>("business")),
                #[cfg(feature = "tokio")]
                Facade::Async => retry.asynchronous().run(|| async { Err::<(), _>("business") }).await,
            };
            let error: RetryError<&'static str> = result.expect_err("callback terminal");
            let RetryFailure::CallbackFailed { callback, .. } = error.failure() else {
                panic!("must preserve callback terminal");
            };
            assert_eq!(callback.phase(), phase);
            assert_eq!(callback.index(), 1);
            assert_eq!(callback.panic(), &RetryPanic::NonString);
            assert_eq!(
                callback.callback(),
                if phase == RetryCallbackPhase::RuleDecision {
                    RetryCallbackKind::Rule
                } else {
                    RetryCallbackKind::Observer
                }
            );
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(later.load(Ordering::SeqCst), 0);
            assert_eq!(completed.load(Ordering::SeqCst), 1);
            if phase == RetryCallbackPhase::BeforeAttempt {
                assert_eq!(error.context().attempts(), 0);
                assert!(error.last_error().is_none());
            } else {
                assert_eq!(error.context().attempts(), 1);
                assert_eq!(error.last_error(), Some(&"business"));
            }
        }
    }
}

#[tokio::test]
async fn control_payload_nonrecursive_drop_panic() {
    run_matrix(false).await;
}

#[tokio::test]
async fn control_payload_recursive_drop_panic() {
    run_matrix(true).await;
}
