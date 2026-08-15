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
