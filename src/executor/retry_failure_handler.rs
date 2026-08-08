// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry failure handling.
//!
//! Runners call this object only after an attempt has failed and operation
//! elapsed time has been recorded. The handler is the retry-flow "decision
//! pipeline": enrich context, ask listeners, apply default policy, enforce hard
//! limits, select delay, notify retry-scheduled listeners, and finally return
//! either "retry after this delay" or a terminal [`RetryError`].

use std::time::Duration;

use super::retry_failure_policy::RetryFailurePolicy;
use super::retry_flow_action::RetryFlowAction;
use super::retry_flow_state::RetryFlowState;
use crate::AttemptFailure;
use crate::AttemptFailureDecision;
use crate::RetryContext;
use crate::RetryError;
use crate::RetryErrorReason;
use crate::RetryOptions;
use crate::RetryRandomSource;
use crate::event::RetryEvents;

/// Handles state transitions after one failed attempt.
pub(in crate::executor) struct RetryFailureHandler<'a, E> {
    /// Retry options used for limits and delay selection.
    options: &'a RetryOptions,
    /// Event dispatcher used for hints and listeners.
    events: &'a RetryEvents<E>,
    /// Random source used for delay and jitter selection.
    random_source: &'a dyn RetryRandomSource,
    /// Default failure policy.
    policy: RetryFailurePolicy<'a>,
}

impl<'a, E> RetryFailureHandler<'a, E> {
    /// Creates a failure handler.
    ///
    /// # Arguments
    /// - `options`: Retry options used for limits and delay selection.
    /// - `events`: Event dispatcher used for hints and listeners.
    /// - `random_source`: Source used for delay and jitter selection.
    ///
    /// # Returns
    /// A failure handler for one retry policy.
    #[inline(always)]
    pub(in crate::executor) fn new(
        options: &'a RetryOptions,
        events: &'a RetryEvents<E>,
        random_source: &'a dyn RetryRandomSource,
    ) -> Self {
        Self {
            options,
            events,
            random_source,
            policy: RetryFailurePolicy::new(options),
        }
    }

    /// Handles one failed attempt.
    ///
    /// # Arguments
    /// - `state`: Retry-flow state after the failed attempt has been recorded.
    /// - `failure`: Attempt failure.
    /// - `context`: Context captured after the failed attempt.
    /// - `retry_block_reason`: Terminal reason that prevents another attempt.
    ///
    /// # Returns
    /// A retry action selected from listeners and configured limits.
    pub(in crate::executor) fn handle(
        &self,
        state: &RetryFlowState<'_, E>,
        failure: AttemptFailure<E>,
        context: RetryContext,
        retry_block_reason: Option<RetryErrorReason>,
    ) -> RetryFlowAction<E> {
        // Failure listeners may force Retry, RetryAfter, or Abort. If they all
        // choose UseDefault, RetryFailurePolicy applies the library defaults
        // for timeout, panic, executor, and ordinary operation errors.
        let (hint, listener_decision, context) =
            self.observe_failure(state, &failure, context);
        let decision = self.policy.resolve(listener_decision, &failure);
        if decision == AttemptFailureDecision::Abort {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::Aborted,
                Some(failure),
                context,
            ));
        }

        // Some runners have extra safety stops that are not policy choices.
        // For example, worker execution refuses to start another attempt while
        // a timed-out worker is still running.
        if let Some(reason) = retry_block_reason {
            return RetryFlowAction::Finished(RetryError::new(
                reason,
                Some(failure),
                context,
            ));
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

        if let Some(reason) = self.options.elapsed_error_reason(
            context.operation_elapsed(),
            context.total_elapsed(),
        ) {
            return RetryFlowAction::Finished(RetryError::new(
                reason,
                Some(failure),
                context,
            ));
        }

        // Delay selection order is centralized in RetryOptions. Explicit
        // RetryAfter wins, then retry-after hints when the default policy is
        // used, then the configured delay and jitter strategy.
        let delay = self.options.retry_delay(
            decision,
            state.attempts(),
            hint,
            self.random_source,
        );
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
        if let Some(reason) = self.options.elapsed_error_reason(
            context.operation_elapsed(),
            context.total_elapsed(),
        ) {
            return RetryFlowAction::Finished(RetryError::new(
                reason,
                Some(failure),
                context,
            ));
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

    /// Builds a terminal error for an elapsed-budget timeout after observation.
    ///
    /// # Arguments
    ///
    /// * `state` - Retry-flow state used to refresh total elapsed time.
    /// * `failure` - Timeout failure produced by the admitted attempt.
    /// * `context` - Context captured immediately after the attempt.
    /// * `reason` - Hard elapsed-budget terminal reason.
    ///
    /// # Returns
    ///
    /// A terminal error that preserves the timeout and ignores listener policy.
    pub(in crate::executor) fn elapsed_timeout_error(
        &self,
        state: &RetryFlowState<'_, E>,
        failure: AttemptFailure<E>,
        context: RetryContext,
        reason: RetryErrorReason,
    ) -> RetryError<E> {
        let (_hint, _listener_decision, context) =
            self.observe_failure(state, &failure, context);
        RetryError::new(reason, Some(failure), context)
    }

    /// Observes one failed attempt before retry policy is applied.
    ///
    /// # Arguments
    ///
    /// * `state` - Retry-flow state used to refresh total elapsed time.
    /// * `failure` - Failure produced by the admitted attempt.
    /// * `context` - Context captured immediately after the attempt.
    ///
    /// # Returns
    ///
    /// The extracted hint, raw listener decision, and refreshed context.
    fn observe_failure(
        &self,
        state: &RetryFlowState<'_, E>,
        failure: &AttemptFailure<E>,
        context: RetryContext,
    ) -> (Option<Duration>, AttemptFailureDecision, RetryContext) {
        // Hints run before failure listeners so listeners can inspect the
        // extracted value while choosing a policy override.
        let hint = self.events.retry_after_hint(failure, &context);
        let context = context
            .with_retry_after_hint(hint)
            .with_total_elapsed(state.total_elapsed());
        let listener_decision = self.events.failure_decision(failure, &context);
        let context = context.with_total_elapsed(state.total_elapsed());
        (hint, listener_decision, context)
    }
}
