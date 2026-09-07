// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal panic-isolating observer collection.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;

use crate::AttemptFailure;
use crate::BackoffStep;
use crate::RetryCallbackFailure;
use crate::RetryCallbackKind;
use crate::RetryCallbackPhase;
use crate::RetryContext;
use crate::RetryErrorReason;
use crate::internal::retry_panic_from_payload;
use crate::observer::RetryObserver;

/// Ordered observer collection.
/// # Type Parameters
/// - `E`: Application error borrowed by the shared lifecycle observers.
pub(crate) struct RetryObservers<E> {
    /// Ordered shared observer objects; cloning copies references without
    /// cloning E.
    observers: Arc<[Arc<dyn RetryObserver<E>>]>,
}

/// Clones the ordered observer references without cloning the operation error.
impl<E> Clone for RetryObservers<E> {
    ///
    /// Clones the ordered observer references without duplicating callback
    /// objects.
    ///
    /// # Returns
    /// A collection sharing callbacks but owning its independent vector.
    #[inline]
    fn clone(&self) -> Self {
        Self {
            observers: Arc::clone(&self.observers),
        }
    }
}

impl<E> Default for RetryObservers<E> {
    ///
    /// Creates an empty ordered observer collection.
    ///
    /// # Returns
    /// A collection with no registered callbacks and no vector allocation.
    #[inline]
    fn default() -> Self {
        Self {
            observers: Arc::from([]),
        }
    }
}

impl<E: 'static> RetryObservers<E> {
    /// Appends one observer.
    ///
    /// # Type Parameters
    /// - `O`: Thread-safe observer stored behind a shared pointer.
    ///
    /// # Parameters
    /// - `observer`: Callback consumed and appended after all existing
    ///   registrations.
    #[inline]
    pub(crate) fn push<O>(&mut self, observer: O)
    where
        O: RetryObserver<E>,
    {
        let mut observers: Vec<_> = self.observers.iter().cloned().collect();
        observers.push(Arc::new(observer));
        self.observers = observers.into();
    }

    pub(crate) fn push_shared(&mut self, observer: Arc<dyn RetryObserver<E>>) {
        let mut observers: Vec<_> = self.observers.iter().cloned().collect();
        observers.push(observer);
        self.observers = observers.into();
    }

    /// Notifies observers before an attempt and stops on the first panic.
    ///
    /// # Parameters
    /// - `context`: Pre-admission snapshot with an upcoming attempt overlay.
    ///
    /// # Returns
    /// Unit when every observer returned normally.
    ///
    /// # Errors
    /// Returns the first captured observer panic; later observers do not run.
    #[inline(always)]
    pub(crate) fn try_before_attempt(&self, context: &RetryContext) -> Result<(), RetryCallbackFailure> {
        self.try_each(RetryCallbackPhase::BeforeAttempt, |observer| {
            observer.on_before_attempt(context)
        })
    }

    /// Notifies observers of an attempt failure and stops on the first panic.
    ///
    /// # Parameters
    /// - `failure`: Committed failure borrowed by each observer.
    /// - `context`: Snapshot after operation accounting and before rules.
    ///
    /// # Returns
    /// Unit when every observer returned normally.
    ///
    /// # Errors
    /// Returns the first captured observer panic; later observers do not run.
    #[inline(always)]
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
    ///
    /// # Parameters
    /// - `backoff`: Selected delay and source, currently eligible for
    ///   continuation.
    /// - `context`: Scheduling snapshot, without a guarantee of admission.
    ///
    /// # Returns
    /// Unit when every observer returned normally.
    ///
    /// # Errors
    /// Returns the first captured observer panic; later observers do not run.
    #[inline(always)]
    pub(crate) fn try_retry_scheduled(
        &self,
        backoff: &BackoffStep,
        context: &RetryContext,
    ) -> Result<(), RetryCallbackFailure> {
        self.try_each(RetryCallbackPhase::RetryScheduled, |observer| {
            observer.on_retry_scheduled(backoff, context)
        })
    }

    /// Notifies every observer of success using the frozen context.
    ///
    /// Returns all caught panics in registration order without stopping later
    /// observers or changing the successful result.
    ///
    /// # Parameters
    /// - `context`: Frozen success snapshot, excluding completion time.
    ///
    /// # Returns
    /// Every captured completion panic in registration order; no allocation
    /// when no callback panics. The original outcome is not changed.
    #[inline(always)]
    pub(crate) fn notify_success(&self, context: &RetryContext) -> Vec<RetryCallbackFailure> {
        self.notify_each(RetryCallbackPhase::Success, |observer| observer.on_success(context))
    }

    /// Notifies every observer of the original terminal failure and context.
    ///
    /// Returns all caught panics in registration order without replacing the
    /// failure or invoking any retry controls.
    ///
    /// # Parameters
    /// - `failure`: Frozen original terminal failure.
    /// - `context`: Frozen context, including zero-attempt failures.
    ///
    /// # Returns
    /// Every captured completion panic in registration order; no allocation
    /// when no callback panics. The original outcome is not changed.
    #[inline(always)]
    pub(crate) fn notify_terminal_failure(
        &self,
        reason: &RetryErrorReason,
        context: &RetryContext,
    ) -> Vec<RetryCallbackFailure> {
        self.notify_each(RetryCallbackPhase::TerminalFailure, |observer| {
            observer.on_terminal_failure(reason, context)
        })
    }

    /// Invokes all completion callbacks, collecting each panic independently.
    ///
    /// The returned vector allocates only when a callback panics. Indices
    /// retain the original observer registration order.
    ///
    /// # Type Parameters
    /// - `F`: Invoker selecting one completion method on each observer.
    ///
    /// # Parameters
    /// - `phase`: Completion phase assigned to diagnostics.
    /// - `callback`: Synchronous invoker; each panic is captured independently.
    ///
    /// # Returns
    /// An ordered vector of panics; later observers run even after an earlier
    /// panic.
    fn notify_each<F>(&self, phase: RetryCallbackPhase, mut callback: F) -> Vec<RetryCallbackFailure>
    where
        F: FnMut(&dyn RetryObserver<E>),
    {
        let mut failures = Vec::new();
        for (index, observer) in self.observers.iter().enumerate() {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| callback(observer.as_ref()))) {
                let panic = retry_panic_from_payload(payload);
                failures.push(RetryCallbackFailure::new(
                    RetryCallbackKind::Observer,
                    index,
                    phase,
                    panic,
                ));
            }
        }
        failures
    }

    /// Invokes one observer phase in registration order.
    ///
    /// Returns a structured failure for the first panicking observer without
    /// invoking any later observer.
    ///
    /// # Type Parameters
    /// - `F`: Invoker selecting one control method on each observer.
    ///
    /// # Parameters
    /// - `phase`: Control phase assigned to the first failure.
    /// - `callback`: Synchronous invoker stopped at its first panic.
    ///
    /// # Returns
    /// Unit if all registered callbacks return normally.
    ///
    /// # Errors
    /// Returns the first panic with observer kind, index, phase and decoded
    /// payload.
    fn try_each<F>(&self, phase: RetryCallbackPhase, mut callback: F) -> Result<(), RetryCallbackFailure>
    where
        F: FnMut(&dyn RetryObserver<E>),
    {
        for (index, observer) in self.observers.iter().enumerate() {
            catch_unwind(AssertUnwindSafe(|| callback(observer.as_ref()))).map_err(|payload| {
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
