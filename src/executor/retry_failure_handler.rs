/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry failure handling.

use crate::event::RetryEvents;
use crate::{
    AttemptFailure,
    AttemptFailureDecision,
    RetryContext,
    RetryError,
    RetryErrorReason,
    RetryOptions,
};

use super::retry_failure_policy::RetryFailurePolicy;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;

/// Handles state transitions after one failed attempt.
pub(in crate::executor) struct RetryFailureHandler<'a, E> {
    /// Retry options used for limits and delay selection.
    options: &'a RetryOptions,
    /// Event dispatcher used for hints and listeners.
    events: &'a RetryEvents<E>,
    /// Default failure policy.
    policy: RetryFailurePolicy<'a>,
}

impl<'a, E> RetryFailureHandler<'a, E> {
    /// Creates a failure handler.
    ///
    /// # Parameters
    /// - `options`: Retry options used for limits and delay selection.
    /// - `events`: Event dispatcher used for hints and listeners.
    ///
    /// # Returns
    /// A failure handler for one retry policy.
    #[inline]
    pub(in crate::executor) fn new(options: &'a RetryOptions, events: &'a RetryEvents<E>) -> Self {
        Self {
            options,
            events,
            policy: RetryFailurePolicy::new(options),
        }
    }

    /// Handles one failed attempt.
    ///
    /// # Parameters
    /// - `state`: Retry-flow state after the failed attempt has been recorded.
    /// - `failure`: Attempt failure.
    /// - `context`: Context captured after the failed attempt.
    /// - `retry_block_reason`: Terminal reason that prevents another attempt.
    ///
    /// # Returns
    /// A retry action selected from listeners and configured limits.
    pub(in crate::executor) fn handle(
        &self,
        state: &RetryFlowState<E>,
        failure: AttemptFailure<E>,
        context: RetryContext,
        retry_block_reason: Option<RetryErrorReason>,
    ) -> RetryFlowAction<E> {
        let hint = self.events.retry_after_hint(&failure, &context);
        let context = context
            .with_retry_after_hint(hint)
            .with_total_elapsed(state.total_elapsed());

        let decision = self
            .policy
            .resolve(self.events.failure_decision(&failure, &context), &failure);
        let context = context.with_total_elapsed(state.total_elapsed());
        if decision == AttemptFailureDecision::Abort {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::Aborted,
                Some(failure),
                context,
            ));
        }

        if let Some(reason) = retry_block_reason {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        if state.attempts() >= self.options.max_attempts() {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::AttemptsExceeded,
                Some(failure),
                context,
            ));
        }

        if let Some(reason) = self
            .options
            .elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        let delay = self.options.retry_delay(decision, state.attempts(), hint);
        let context = context
            .with_total_elapsed(state.total_elapsed())
            .with_next_delay(delay);
        if self
            .options
            .retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay)
        {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        self.events.retry_scheduled(&failure, &context);
        let context = context.with_total_elapsed(state.total_elapsed());
        if let Some(reason) = self
            .options
            .elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }
        if self
            .options
            .retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay)
        {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        RetryFlowAction::Retry { delay, failure }
    }
}
