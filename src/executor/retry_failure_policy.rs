/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Default retry failure policy.
//!
//! Listener decisions and built-in defaults are intentionally separated. The
//! event dispatcher decides what listeners requested; this policy only fills in
//! the answer when listeners return [`AttemptFailureDecision::UseDefault`].

use crate::{
    AttemptFailure,
    AttemptFailureDecision,
    AttemptTimeoutPolicy,
    RetryOptions,
};

/// Resolves listener decisions into concrete retry failure decisions.
pub(in crate::executor) struct RetryFailurePolicy<'a> {
    /// Retry options that define timeout policy.
    options: &'a RetryOptions,
}

impl<'a> RetryFailurePolicy<'a> {
    /// Creates a failure policy.
    ///
    /// # Parameters
    /// - `options`: Retry options that define timeout policy.
    ///
    /// # Returns
    /// A failure policy using the provided options.
    #[inline]
    pub(in crate::executor) fn new(options: &'a RetryOptions) -> Self {
        Self { options }
    }

    /// Resolves the effective failure decision after applying default policy.
    ///
    /// # Parameters
    /// - `decision`: Decision returned by failure listeners.
    /// - `failure`: Attempt failure being handled.
    ///
    /// # Returns
    /// A concrete decision for timeout, panic, and executor failures when
    /// listeners used the default.
    pub(in crate::executor) fn resolve<E>(
        &self,
        decision: AttemptFailureDecision,
        failure: &AttemptFailure<E>,
    ) -> AttemptFailureDecision {
        if decision != AttemptFailureDecision::UseDefault {
            return decision;
        }
        // A configured attempt timeout is the only timeout that can follow the
        // configured timeout policy. Timeouts selected by elapsed budgets are
        // handled earlier as terminal elapsed-budget errors.
        if matches!(failure, AttemptFailure::Timeout)
            && let Some(attempt_timeout) = self.options.attempt_timeout()
        {
            match attempt_timeout.policy() {
                AttemptTimeoutPolicy::Retry => AttemptFailureDecision::Retry,
                AttemptTimeoutPolicy::Abort => AttemptFailureDecision::Abort,
            }
        } else if matches!(
            failure,
            AttemptFailure::Panic(_) | AttemptFailure::Executor(_)
        ) {
            // Panics and executor failures are infrastructure failures by
            // default. Callers can still override them explicitly with an
            // on_failure listener.
            AttemptFailureDecision::Abort
        } else {
            AttemptFailureDecision::UseDefault
        }
    }
}
