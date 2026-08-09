// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Internal panic-isolating observer collection.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use crate::AttemptFailure;
use crate::BackoffStep;
use crate::RetryContext;

use super::RetryDiagnostic;
use super::RetryDiagnosticKind;
use super::RetryObserver;
use super::RetryOutcomeKind;

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

    /// Notifies observers before an attempt.
    pub(crate) fn attempt_started(&self, context: &RetryContext) {
        self.each(|observer| observer.on_attempt_started(context));
    }

    /// Notifies observers of an attempt failure.
    pub(crate) fn attempt_failed(&self, failure: &AttemptFailure<E>, context: &RetryContext) {
        self.each(|observer| observer.on_attempt_failed(failure, context));
    }

    /// Notifies observers of a selected retry.
    pub(crate) fn retry_scheduled(&self, backoff: &BackoffStep, context: &RetryContext) {
        self.each(|observer| observer.on_retry_scheduled(backoff, context));
    }

    /// Notifies observers of the terminal outcome.
    pub(crate) fn finished(&self, outcome: RetryOutcomeKind, context: &RetryContext) {
        self.each(|observer| observer.on_finished(outcome, context));
    }

    /// Notifies all observers of callback diagnostics.
    pub(crate) fn diagnostic(
        &self,
        diagnostic: &RetryDiagnostic,
        context: &RetryContext,
        failed_index: Option<usize>,
    ) {
        for (index, observer) in self.observers.iter().enumerate() {
            if Some(index) == failed_index {
                continue;
            }
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                observer.on_diagnostic(diagnostic, context);
            }));
        }
    }

    fn each<F>(&self, mut callback: F)
    where
        F: FnMut(&dyn RetryObserver<E>),
    {
        for (index, observer) in self.observers.iter().enumerate() {
            if std::panic::catch_unwind(AssertUnwindSafe(|| callback(observer.as_ref()))).is_err() {
                let diagnostic = RetryDiagnostic::new(RetryDiagnosticKind::ObserverPanicked, index);
                let context = RetryContext::new(0, 1);
                self.diagnostic(&diagnostic, &context, Some(index));
            }
        }
    }
}
