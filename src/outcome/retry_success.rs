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
///
/// # Type Parameters
/// - `T`: Owned successful value; inspecting diagnostics does not constrain
///   this type.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryConfig;
/// use qubit_retry::RetrySuccess;
///
/// let config = RetryConfig::<&str>::builder().max_attempts(3).build()?;
/// let success: RetrySuccess<u32> = Retry::new(&config).run(|| Ok(7)).unwrap();
/// let (value, context, diagnostics) = success.into_parts();
/// assert_eq!(value, 7);
/// assert_eq!(context.attempts(), 1);
/// assert!(diagnostics.is_empty());
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrySuccess<T> {
    /// Owned successful operation value.
    value: T,
    /// Frozen context at successful completion.
    context: RetryContext,
    /// Panics raised while notifying completion observers.
    completion_callback_failures: Vec<RetryCallbackFailure>,
}

impl<T> RetrySuccess<T> {
    /// Creates a successful result with no completion diagnostics.
    ///
    /// # Parameters
    /// - `value`: Successful operation value.
    /// - `context`: Frozen terminal context.
    ///
    /// # Returns
    /// A success with an empty diagnostic collection.
    #[inline]
    pub(crate) fn new(value: T, context: RetryContext) -> Self {
        Self {
            value,
            context,
            completion_callback_failures: Vec::new(),
        }
    }

    /// Returns the successful operation value.
    ///
    /// # Returns
    /// The successful value borrowed without cloning.
    #[inline(always)]
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns the final retry context.
    ///
    /// # Returns
    /// The frozen terminal context borrowed from this result.
    #[inline(always)]
    #[must_use = "inspect the final retry context"]
    pub fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns completion observer panics in registration order.
    ///
    /// An empty slice means no completion callback panicked. These diagnostics
    /// do not change the operation result or the frozen terminal context.
    ///
    /// # Returns
    /// The ordered diagnostics; an empty slice means no completion callback
    /// panicked.
    #[must_use = "inspect completion observer diagnostics"]
    #[inline(always)]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    /// Consumes this result, preserving its terminal data and diagnostics.
    ///
    /// Returns the (value, context, completion callback failures) triple.
    ///
    /// # Returns
    /// The owned value, context, and diagnostics, without information loss.
    #[must_use = "consume the terminal result, context and completion diagnostics"]
    #[inline(always)]
    pub fn into_parts(self) -> (T, RetryContext, Vec<RetryCallbackFailure>) {
        (self.value, self.context, self.completion_callback_failures)
    }

    /// Consumes this result and returns the successful value.
    ///
    /// Discards the context and completion callback diagnostics.
    ///
    /// # Returns
    /// The owned successful value; context and diagnostics are dropped.
    #[inline(always)]
    #[must_use]
    pub fn into_value_discarding_diagnostics(self) -> T {
        self.value
    }

    /// Attaches completion diagnostics after the final result is frozen.
    ///
    /// # Parameters
    /// - `failures`: Ordered completion diagnostics replacing the empty
    ///   collection.
    #[inline(always)]
    pub(crate) fn set_completion_callback_failures(&mut self, failures: Vec<RetryCallbackFailure>) {
        self.completion_callback_failures = failures;
    }
}
