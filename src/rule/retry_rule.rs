// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry rule trait.

use super::RetryDecision;
use crate::AttemptFailure;
use crate::RetryContext;

/// Decides whether one attempt failure should be retried.
///
/// # Type Parameters
/// - `E`: Application error observed by reference, without a cloning
///   requirement.
///
/// Rules are shared across runs and must synchronize mutable state. The first
/// non-UseDefault decision wins but cannot bypass continuation limits.
pub trait RetryRule<E>: Send + Sync + 'static {
    /// Classifies a committed attempt failure before retry scheduling.
    ///
    /// # Parameters
    /// - `failure`: Last committed application error, timeout, or captured
    ///   panic.
    /// - `context`: Current accounting before delay selection.
    ///
    /// # Returns
    /// UseDefault to delegate, Abort to stop, or a retry request still subject
    /// to cancellation, timeout, and continuation budgets.
    #[must_use = "the decision controls retry continuation"]
    fn decide(&self, failure: &AttemptFailure<E>, context: &RetryContext) -> RetryDecision;
}

impl<E, F> RetryRule<E> for F
where
    F: Fn(&AttemptFailure<E>, &RetryContext) -> RetryDecision + Send + Sync + 'static,
{
    /// Forwards failure classification to this closure.
    ///
    /// # Parameters
    /// - `failure`: Committed attempt failure borrowed for this call.
    /// - `context`: Current accounting before delay selection.
    ///
    /// # Returns
    /// The closure's decision, without evaluating admission budgets.
    ///
    /// # Panics
    /// Propagates closure panic to the executor's rule capture boundary.
    #[inline(always)]
    fn decide(&self, failure: &AttemptFailure<E>, context: &RetryContext) -> RetryDecision {
        self(failure, context)
    }
}
