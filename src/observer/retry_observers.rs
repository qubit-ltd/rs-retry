// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal panic-isolating observer collection.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use super::RetryObserver;
use super::retry_panic_from_payload;
use crate::AttemptFailure;
use crate::BackoffStep;
use crate::RetryCallbackFailure;
use crate::RetryCallbackKind;
use crate::RetryCallbackPhase;
use crate::RetryContext;

/// Ordered observer collection.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct RetryObservers<E> {
    observers: Vec<Arc<dyn RetryObserver<E>>>,
}

impl<E> Default for RetryObservers<E> {
    fn default() -> Self {
        Self {
            observers: Vec::new(),
        }
    }
}

impl<E: 'static> RetryObservers<E> {
    /// Appends one observer.
    pub(crate) fn push<O>(&mut self, observer: O)
    where
        O: RetryObserver<E>,
    {
        self.observers.push(Arc::new(observer));
    }

    /// Notifies observers before an attempt and stops on the first panic.
    pub(crate) fn try_attempt_started(
        &self,
        context: &RetryContext,
    ) -> Result<(), RetryCallbackFailure> {
        self.try_each(RetryCallbackPhase::AttemptStarted, |observer| {
            observer.on_attempt_started(context)
        })
    }

    /// Notifies observers of an attempt failure and stops on the first panic.
    pub(crate) fn try_attempt_failed(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> Result<(), RetryCallbackFailure> {
        self.try_each(RetryCallbackPhase::AttemptFailed, |observer| {
            observer.on_attempt_failed(failure, context)
        })
    }

    /// Notifies observers of a selected retry and stops on the first panic.
    pub(crate) fn try_retry_scheduled(
        &self,
        backoff: &BackoffStep,
        context: &RetryContext,
    ) -> Result<(), RetryCallbackFailure> {
        self.try_each(RetryCallbackPhase::RetryScheduled, |observer| {
            observer.on_retry_scheduled(backoff, context)
        })
    }

    /// Invokes one observer phase in registration order.
    ///
    /// Returns a structured failure for the first panicking observer without
    /// invoking any later observer.
    fn try_each<F>(
        &self,
        phase: RetryCallbackPhase,
        mut callback: F,
    ) -> Result<(), RetryCallbackFailure>
    where
        F: FnMut(&dyn RetryObserver<E>),
    {
        for (index, observer) in self.observers.iter().enumerate() {
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                callback(observer.as_ref())
            }))
            .map_err(|payload| {
                RetryCallbackFailure::new(
                    RetryCallbackKind::Observer,
                    index,
                    phase,
                    retry_panic_from_payload(payload),
                )
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::panic::panic_any;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::RetryObservers;
    use crate::AttemptFailure;
    use crate::BackoffDelaySource;
    use crate::BackoffStep;
    use crate::RetryCallbackKind;
    use crate::RetryCallbackPhase;
    use crate::RetryContext;
    use crate::RetryObserver;
    use crate::RetryPanic;

    #[derive(Clone, Copy, Debug)]
    enum PanicPayload {
        StaticStr,
        String,
        NonString,
    }

    impl PanicPayload {
        /// Panics with the payload represented by this test case.
        fn raise(self) -> ! {
            match self {
                Self::StaticStr => panic!("static observer panic"),
                Self::String => panic_any(String::from("owned observer panic")),
                Self::NonString => panic_any(29_u32),
            }
        }

        /// Returns the stable payload expected from this test case.
        fn expected(self) -> RetryPanic {
            match self {
                Self::StaticStr => {
                    RetryPanic::StaticStr("static observer panic")
                }
                Self::String => {
                    RetryPanic::String(String::from("owned observer panic"))
                }
                Self::NonString => RetryPanic::NonString,
            }
        }
    }

    struct NoopObserver;

    impl RetryObserver<&'static str> for NoopObserver {}

    struct PanickingObserver {
        phase: RetryCallbackPhase,
        payload: PanicPayload,
        calls: Arc<AtomicUsize>,
    }

    impl RetryObserver<&'static str> for PanickingObserver {
        fn on_attempt_started(&self, _context: &RetryContext) {
            if self.phase == RetryCallbackPhase::AttemptStarted {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.payload.raise();
            }
        }

        fn on_attempt_failed(
            &self,
            _failure: &AttemptFailure<&'static str>,
            _context: &RetryContext,
        ) {
            if self.phase == RetryCallbackPhase::AttemptFailed {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.payload.raise();
            }
        }

        fn on_retry_scheduled(
            &self,
            _backoff: &BackoffStep,
            _context: &RetryContext,
        ) {
            if self.phase == RetryCallbackPhase::RetryScheduled {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.payload.raise();
            }
        }
    }

    struct CountingObserver {
        phase: RetryCallbackPhase,
        calls: Arc<AtomicUsize>,
    }

    impl RetryObserver<&'static str> for CountingObserver {
        fn on_attempt_started(&self, _context: &RetryContext) {
            if self.phase == RetryCallbackPhase::AttemptStarted {
                self.calls.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn on_attempt_failed(
            &self,
            _failure: &AttemptFailure<&'static str>,
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

    /// Verifies exact observer panic attribution for every phase and payload.
    #[test]
    fn test_try_observer_callbacks_report_exact_callback_failure() {
        let phases = [
            RetryCallbackPhase::AttemptStarted,
            RetryCallbackPhase::AttemptFailed,
            RetryCallbackPhase::RetryScheduled,
        ];
        let payloads = [
            PanicPayload::StaticStr,
            PanicPayload::String,
            PanicPayload::NonString,
        ];

        for phase in phases {
            for payload in payloads {
                let panicking_calls = Arc::new(AtomicUsize::new(0));
                let later_calls = Arc::new(AtomicUsize::new(0));
                let mut observers = RetryObservers::default();
                observers.push(NoopObserver);
                observers.push(PanickingObserver {
                    phase,
                    payload,
                    calls: Arc::clone(&panicking_calls),
                });
                observers.push(CountingObserver {
                    phase,
                    calls: Arc::clone(&later_calls),
                });
                let context = RetryContext::new(1, 2);
                let failure = AttemptFailure::Error("operation failed");
                let backoff = BackoffStep::new(
                    1,
                    Duration::ZERO,
                    Duration::ZERO,
                    BackoffDelaySource::Policy,
                );

                let callback_failure = match phase {
                    RetryCallbackPhase::AttemptStarted => {
                        observers.try_attempt_started(&context)
                    }
                    RetryCallbackPhase::AttemptFailed => {
                        observers.try_attempt_failed(&failure, &context)
                    }
                    RetryCallbackPhase::RetryScheduled => {
                        observers.try_retry_scheduled(&backoff, &context)
                    }
                    RetryCallbackPhase::RuleDecision => {
                        unreachable!("rule decisions are not observer phases")
                    }
                }
                .expect_err("the second observer should panic");

                assert_eq!(
                    callback_failure.callback(),
                    RetryCallbackKind::Observer,
                );
                assert_eq!(callback_failure.index(), 1);
                assert_eq!(callback_failure.phase(), phase);
                assert_eq!(callback_failure.panic(), &payload.expected());
                assert_eq!(panicking_calls.load(Ordering::SeqCst), 1);
                assert_eq!(later_calls.load(Ordering::SeqCst), 0);
            }
        }
    }
}
