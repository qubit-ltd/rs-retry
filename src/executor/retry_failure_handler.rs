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
//!
//! Runners call this object only after an attempt has failed and operation
//! elapsed time has been recorded. The handler is the retry-flow "decision
//! pipeline": enrich context, ask listeners, apply default policy, enforce hard
//! limits, select delay, notify retry-scheduled listeners, and finally return
//! either "retry after this delay" or a terminal [`RetryError`].

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
        // Retry-after hints are extracted before failure listeners so custom
        // listener decisions can inspect the hint and still override the
        // default delay selection if needed.
        let hint = self.events.retry_after_hint(&failure, &context);
        let context = context
            .with_retry_after_hint(hint)
            .with_total_elapsed(state.total_elapsed());

        // Failure listeners may force Retry, RetryAfter, or Abort. If they all
        // choose UseDefault, RetryFailurePolicy applies the library defaults for
        // timeout, panic, executor, and ordinary operation errors.
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

        // Some runners have extra safety stops that are not policy choices.
        // For example, worker execution refuses to start another attempt while a
        // timed-out worker is still running.
        if let Some(reason) = retry_block_reason {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        // Hard limits are checked after listeners so callers can still observe
        // the failure that exhausted the retry flow.
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

        // Delay selection order is centralized in RetryOptions. Explicit
        // RetryAfter wins, then retry-after hints when the default policy is
        // used, then the configured delay and jitter strategy.
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
        // on_retry listeners are observational, but they run before the sleep
        // and can consume total elapsed budget. Re-check limits afterwards so
        // the executor never sleeps past the total budget.
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
