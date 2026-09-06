// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Successful retry execution result.

use crate::RetryCallbackFailure;
use crate::RetryContext;

/// Successful retry value together with the final retry context.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrySuccess<T> {
    value: T,
    context: RetryContext,
    /// Panics raised while notifying completion observers.
    completion_callback_failures: Vec<RetryCallbackFailure>,
}

impl<T> RetrySuccess<T> {
    /// Creates a successful result with no completion diagnostics.
    #[inline]
    pub(crate) fn new(value: T, context: RetryContext) -> Self {
        Self {
            value,
            context,
            completion_callback_failures: Vec::new(),
        }
    }

    /// Returns completion observer panics in registration order.
    ///
    /// An empty slice means no completion callback panicked. These diagnostics
    /// do not change the operation result or the frozen terminal context.
    #[must_use]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    /// Attaches completion diagnostics after the final result is frozen.
    pub(crate) fn set_completion_callback_failures(&mut self, failures: Vec<RetryCallbackFailure>) {
        self.completion_callback_failures = failures;
    }

    /// Consumes this result, preserving its terminal data and diagnostics.
    ///
    /// Returns the (value, context, completion callback failures) triple.
    #[must_use = "consume the terminal result, context and completion diagnostics"]
    pub fn into_parts_with_diagnostics(self) -> (T, RetryContext, Vec<RetryCallbackFailure>) {
        (self.value, self.context, self.completion_callback_failures)
    }

    /// Returns the successful operation value.
    #[inline(always)]
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns the final retry context.
    #[inline(always)]
    #[must_use = "inspect the final retry context"]
    pub fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Consumes this result and returns the successful value.
    ///
    /// Discards the context and completion callback diagnostics.
    #[inline(always)]
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Consumes this result and returns its value and final context.
    ///
    /// Discards completion callback diagnostics; use
    /// [`Self::into_parts_with_diagnostics`] to retain them.
    #[inline(always)]
    #[must_use = "consume the value and final retry context"]
    pub fn into_parts(self) -> (T, RetryContext) {
        (self.value, self.context)
    }
}
