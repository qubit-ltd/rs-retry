// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Retry rule trait.

use super::RetryDecision;
use crate::AttemptFailure;
use crate::RetryContext;

/// Decides whether one attempt failure should be retried.
pub trait RetryRule<E>: Send + Sync + 'static {
    /// Returns one decision for the failure.
    fn decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> RetryDecision;
}

impl<E, F> RetryRule<E> for F
where
    F: Fn(&AttemptFailure<E>, &RetryContext) -> RetryDecision
        + Send
        + Sync
        + 'static,
{
    fn decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> RetryDecision {
        self(failure, context)
    }
}
