// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent retry-flow decisions shared by executor facades.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;

use super::super::Retry;
use super::EffectiveTimeout;
use super::RetryFlowState;
use crate::AttemptFailure;
use crate::RetryCancellationPhase;
use crate::RetryCancellationToken;
use crate::RetryContext;
use crate::RetryDecision;
use crate::RetryError;
use crate::RetryFailure;
use crate::RetryInfrastructureFailure;
use crate::RetryLimitKind;
use crate::RetryRandomSource;
use crate::RetryTimeoutScope;
use crate::observer::RetryObservers;
use crate::rule::RetryRules;

/// One admitted attempt together with its effective hard timeout.
#[derive(Clone, Copy)]
pub(crate) struct AttemptPlan {
    /// Runtime-independent timeout selected for this operation.
    timeout: Option<EffectiveTimeout>,
}

impl AttemptPlan {
    /// Returns the effective hard timeout selected for this attempt.
    pub(crate) fn timeout(&self) -> Option<EffectiveTimeout> {
        self.timeout
    }
}

/// One async attempt prepared against an immutable absolute deadline.
#[cfg(feature = "tokio")]
#[derive(Clone, Copy)]
pub(crate) struct PreparedAttemptPlan {
    /// Absolute timeout transaction prepared before timer registration.
    timeout: Option<PreparedTimeout>,
}

#[cfg(feature = "tokio")]
impl PreparedAttemptPlan {
    /// Returns the absolute timer deadline, when this attempt is bounded.
    pub(crate) fn deadline(&self) -> Option<MonotonicInstant> {
        self.timeout.map(|timeout| timeout.deadline)
    }

    /// Returns the boundary responsible for the prepared deadline.
    pub(crate) fn scope(&self) -> Option<RetryTimeoutScope> {
        self.timeout.map(|timeout| timeout.scope)
    }
}

/// Absolute timeout selected from one coherent admission clock sample.
#[cfg(feature = "tokio")]
#[derive(Clone, Copy)]
struct PreparedTimeout {
    /// Fixed deadline registered with the async timer.
    deadline: MonotonicInstant,
    /// Effective duration at the admission sample.
    duration: Duration,
    /// Boundary responsible for the fixed deadline.
    scope: RetryTimeoutScope,
}

/// Runtime work selected after one failed attempt.
pub(crate) struct RetryDirective {
    /// Sleep duration after applying the remaining hard-flow timeout.
    sleep_duration: Duration,
}

impl RetryDirective {
    /// Returns the duration the executor should wait before retrying.
    pub(crate) fn sleep_duration(&self) -> Duration {
        self.sleep_duration
    }
}

/// Owns all runtime-independent decisions and terminal error construction.
pub(crate) struct RetryFlowController<'a, E> {
    /// Attempt, elapsed-budget, and backoff state.
    state: RetryFlowState<'a>,
    /// Ordered retry rules.
    rules: &'a RetryRules<E>,
    /// Ordered retry observers.
    observers: &'a RetryObservers<E>,
    /// Last failed attempt retained until success or terminal failure.
    last_failure: Option<AttemptFailure<E>>,
    /// Hard timeout applied to each admitted attempt, when configured.
    attempt_timeout: Option<Duration>,
    /// Current attempt ordinal retained for coherent terminal contexts.
    current_attempt: Option<NonZeroU32>,
    /// Effective timeout attached to the current attempt context.
    current_attempt_timeout: Option<Duration>,
    /// Delay selected by the most recent retry decision.
    next_delay: Option<Duration>,
    /// Retry-after hint selected by the most recent retry decision.
    retry_after_hint: Option<Duration>,
}

impl<'a, E: 'static> RetryFlowController<'a, E> {
    /// Creates a controller from one immutable retry definition and clock sample.
    pub(crate) fn new(
        started_at: MonotonicInstant,
        retry: &'a Retry<E>,
        random_source: Arc<dyn RetryRandomSource>,
        attempt_timeout: Option<Duration>,
        flow_timeout: Option<Duration>,
    ) -> Self {
        Self {
            state: RetryFlowState::new(
                started_at,
                retry.policy(),
                random_source,
                flow_timeout,
            ),
            rules: retry.rules(),
            observers: retry.observers(),
            last_failure: None,
            attempt_timeout,
            current_attempt: None,
            current_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        }
    }

    /// Checks all pre-attempt gates and invokes the attempt-started observers.
    ///
    /// # Errors
    /// Returns a terminal retry error when a timeout, cancellation, continuation
    /// limit, clock failure, or attempt-started observer failure stops the flow.
    /// The upcoming operation is not counted until a facade commits it after
    /// its runtime-specific preparation succeeds.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn before_attempt(
        &mut self,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<MonotonicInstant, RetryError<E>> {
        let now = clock.now();
        self.refresh_or_error(now)?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::BeforeAttempt));
        }
        if let Some(limit) = self.state.continuation_limit() {
            return Err(self.exhausted(limit));
        }

        self.current_attempt = Some(self.state.next_attempt());
        self.current_attempt_timeout = None;
        self.next_delay = None;
        self.retry_after_hint = None;
        let started_context = self.snapshot();
        if let Err(callback) =
            self.observers.try_attempt_started(&started_context)
        {
            return Err(self.callback_failed_after_refresh(
                callback,
                started_context,
                clock,
            ));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::BeforeAttempt));
        }

        let admission_sample = clock.now();
        self.refresh_or_error(admission_sample)?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::BeforeAttempt));
        }
        if let Some(limit) = self.state.continuation_limit() {
            return Err(self.exhausted(limit));
        }
        Ok(admission_sample)
    }

    /// Prepares one async attempt and fixes its absolute timeout deadline.
    ///
    /// # Errors
    /// Returns a terminal retry error when the admission sample observes an
    /// expired timeout, cancellation, continuation limit, or invalid clock
    /// arithmetic. The attempt remains uncommitted on every error path.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    #[cfg(feature = "tokio")]
    pub(crate) fn prepare_async_attempt(
        &mut self,
        admission_sample: MonotonicInstant,
    ) -> Result<PreparedAttemptPlan, RetryError<E>> {
        let timeout = match self.prepare_timeout(admission_sample) {
            Ok(timeout) => timeout,
            Err(error) => return Err(self.inactive_clock_failure(error)),
        };
        self.current_attempt_timeout = timeout.map(|timeout| timeout.duration);
        Ok(PreparedAttemptPlan { timeout })
    }

    /// Commits an admitted attempt after runtime preparation has succeeded.
    ///
    /// # Errors
    /// Returns a terminal retry error when a post-preparation clock sample,
    /// timeout, cancellation, or continuation limit prevents the operation
    /// from starting.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn commit_attempt(
        &mut self,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<AttemptPlan, RetryError<E>> {
        let now = clock.now();
        self.refresh_or_error(now)?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::BeforeAttempt));
        }
        if let Some(limit) = self.state.continuation_limit() {
            return Err(self.exhausted(limit));
        }
        self.state.begin_attempt(now);
        let timeout = self.state.effective_timeout(self.attempt_timeout);
        self.current_attempt_timeout = timeout.map(EffectiveTimeout::duration);
        Ok(AttemptPlan { timeout })
    }

    /// Commits an async attempt after its absolute timer was registered.
    ///
    /// # Errors
    /// Returns a terminal retry error when registration consumed the prepared
    /// deadline or a post-registration clock, cancellation, or limit gate
    /// prevents the operation from starting.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    #[cfg(feature = "tokio")]
    pub(crate) fn commit_prepared_attempt(
        &mut self,
        plan: PreparedAttemptPlan,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<(), RetryError<E>> {
        let now = clock.now();
        self.refresh_or_error(now)?;
        if let Some(timeout) = plan.timeout {
            let deadline_reached =
                match Self::deadline_reached(now, timeout.deadline) {
                    Ok(deadline_reached) => deadline_reached,
                    Err(error) => {
                        return Err(self.inactive_clock_failure(error));
                    }
                };
            if deadline_reached {
                return Err(self.timed_out(timeout.scope));
            }
        }
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::BeforeAttempt));
        }
        if let Some(limit) = self.state.continuation_limit() {
            return Err(self.exhausted(limit));
        }
        self.state.begin_attempt(now);
        Ok(())
    }

    /// Records a failed operation and selects the next runtime action.
    ///
    /// # Errors
    /// Returns a terminal retry error when clock refresh, an observer, a rule,
    /// cancellation, an abort decision, or a continuation limit stops the flow.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn record_failure(
        &mut self,
        failure: AttemptFailure<E>,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<RetryDirective, RetryError<E>> {
        self.last_failure = Some(failure);
        let now = clock.now();
        if let Err(error) = self.state.finish_attempt(now) {
            return Err(self.inactive_clock_failure(error));
        }

        let failed_context = self.snapshot();
        let failure = self
            .last_failure
            .as_ref()
            .expect("recorded failure must remain available to callbacks");
        if let Err(callback) =
            self.observers.try_attempt_failed(failure, &failed_context)
        {
            return Err(self.callback_failed_after_refresh(
                callback,
                failed_context,
                clock,
            ));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::Backoff));
        }
        let now = clock.now();
        self.refresh_or_error(now)?;

        let rule_context = self.snapshot();
        let failure = self
            .last_failure
            .as_ref()
            .expect("recorded failure must remain available to callbacks");
        let decision = match self.rules.try_decide(failure, &rule_context) {
            Ok(decision) => decision,
            Err(callback) => {
                return Err(self.callback_failed_after_refresh(
                    callback,
                    rule_context,
                    clock,
                ));
            }
        };
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::Backoff));
        }
        let failure = self
            .last_failure
            .as_ref()
            .expect("recorded failure must remain available to callbacks");
        let default_timeout = if matches!(decision, RetryDecision::UseDefault) {
            failure.timeout_scope()
        } else {
            None
        };
        let default_panic = matches!(decision, RetryDecision::UseDefault)
            && failure.panic().is_some();
        if let Some(scope) = default_timeout {
            self.refresh_best_effort(clock);
            return Err(self.timed_out(scope));
        }
        if matches!(decision, RetryDecision::Abort) || default_panic {
            self.refresh_best_effort(clock);
            return Err(self.aborted());
        }
        let decision = if matches!(decision, RetryDecision::UseDefault) {
            RetryDecision::Retry
        } else {
            decision
        };
        let now = clock.now();
        self.refresh_or_error(now)?;

        self.retry_after_hint = decision.retry_after_hint();
        let backoff = self.state.next_backoff(decision);
        self.next_delay = Some(backoff.effective_delay());
        let scheduled_context = self.snapshot();
        if let Err(callback) = self
            .observers
            .try_retry_scheduled(&backoff, &scheduled_context)
        {
            return Err(self.callback_failed_after_refresh(
                callback,
                scheduled_context,
                clock,
            ));
        }
        self.clear_current_attempt();
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::Backoff));
        }
        let now = clock.now();
        self.refresh_or_error(now)?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::Backoff));
        }
        if let Some(limit) = self.state.retry_limit(backoff.effective_delay()) {
            return Err(self.exhausted(limit));
        }

        Ok(RetryDirective {
            sleep_duration: self
                .state
                .sleep_duration(backoff.effective_delay()),
        })
    }

    /// Finishes a successful operation and builds its terminal context.
    ///
    /// # Errors
    /// Returns a clock infrastructure failure when the completion sample is
    /// from another domain or precedes the flow or attempt start.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn finish_success(
        &mut self,
        clock: &dyn MonotonicClock,
    ) -> Result<RetryContext, RetryError<E>> {
        let now = clock.now();
        if let Err(error) = self.state.finish_attempt(now) {
            return Err(self.inactive_clock_failure(error));
        }
        self.clear_current_attempt();
        Ok(self.snapshot())
    }

    /// Records infrastructure failure while an operation is still active.
    ///
    /// The terminal context retains the active attempt ordinal and timeout so
    /// callers can identify the runtime work whose completion was not observed.
    pub(crate) fn record_active_infrastructure_failure(
        &mut self,
        failure: RetryInfrastructureFailure,
        now: MonotonicInstant,
    ) -> RetryError<E> {
        let context = match self.state.finish_for_infrastructure(now) {
            Ok(()) => self.snapshot(),
            Err(error) => {
                return self.infrastructure(
                    RetryInfrastructureFailure::Clock {
                        message: error.to_string().into_boxed_str(),
                    },
                    self.snapshot(),
                );
            }
        };
        self.infrastructure(failure, context)
    }

    /// Records infrastructure failure after no operation remains active.
    ///
    /// The pending or completed attempt scope is removed from the terminal
    /// context. Scheduling metadata remains available when the failure occurs
    /// during backoff.
    pub(crate) fn record_inactive_infrastructure_failure(
        &mut self,
        failure: RetryInfrastructureFailure,
        now: MonotonicInstant,
    ) -> RetryError<E> {
        self.clear_current_attempt();
        let context = match self.state.finish_for_infrastructure(now) {
            Ok(()) => self.snapshot(),
            Err(error) => {
                return self.infrastructure(
                    RetryInfrastructureFailure::Clock {
                        message: error.to_string().into_boxed_str(),
                    },
                    self.snapshot(),
                );
            }
        };
        self.infrastructure(failure, context)
    }

    /// Records cancellation while an admitted async operation is active.
    ///
    /// The terminal context retains the attempt ordinal and effective timeout.
    /// If the completion clock sample is invalid, the clock infrastructure
    /// failure takes precedence because no coherent cancellation context can
    /// be constructed.
    pub(crate) fn record_attempt_cancellation(
        &mut self,
        clock: &dyn MonotonicClock,
    ) -> RetryError<E> {
        if let Err(error) = self.state.finish_attempt(clock.now()) {
            return self.inactive_clock_failure(error);
        }
        self.cancelled_with_context(
            RetryCancellationPhase::Attempt,
            self.snapshot(),
        )
    }

    /// Records cancellation while no operation is active during backoff.
    ///
    /// The terminal context retains the last attempt failure and scheduling
    /// metadata. If the clock cannot be refreshed coherently, a clock
    /// infrastructure failure is returned instead.
    pub(crate) fn record_backoff_cancellation(
        &mut self,
        clock: &dyn MonotonicClock,
    ) -> RetryError<E> {
        if let Err(error) = self.state.refresh(clock.now()) {
            return self.inactive_clock_failure(error);
        }
        self.cancelled_with_context(
            RetryCancellationPhase::Backoff,
            self.snapshot(),
        )
    }

    /// Returns whether the optional cancellation token has been cancelled.
    fn is_cancelled(cancellation: Option<&RetryCancellationToken>) -> bool {
        cancellation.is_some_and(RetryCancellationToken::is_cancelled)
    }

    /// Selects an absolute timeout from the current async admission sample.
    #[cfg(feature = "tokio")]
    fn prepare_timeout(
        &self,
        now: MonotonicInstant,
    ) -> Result<Option<PreparedTimeout>, qubit_clock::TimeError> {
        let Some(timeout) = self.state.effective_timeout(self.attempt_timeout)
        else {
            return Ok(None);
        };
        let deadline = match timeout.scope() {
            RetryTimeoutScope::Attempt => {
                now.checked_add(timeout.duration())?
            }
            RetryTimeoutScope::Flow => self
                .state
                .flow_deadline()?
                .expect("a flow-scoped timeout requires a configured deadline"),
        };
        Ok(Some(PreparedTimeout {
            deadline,
            duration: timeout.duration(),
            scope: timeout.scope(),
        }))
    }

    /// Returns whether `now` is at or beyond a prepared same-domain deadline.
    #[cfg(feature = "tokio")]
    fn deadline_reached(
        now: MonotonicInstant,
        deadline: MonotonicInstant,
    ) -> Result<bool, qubit_clock::TimeError> {
        deadline.validate_domain(now.domain())?;
        Ok(now.elapsed_since_origin() >= deadline.elapsed_since_origin())
    }

    /// Refreshes total elapsed time or returns a structured clock failure.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    fn refresh_or_error(
        &mut self,
        now: MonotonicInstant,
    ) -> Result<(), RetryError<E>> {
        if let Err(error) = self.state.refresh(now) {
            return Err(self.inactive_clock_failure(error));
        }
        Ok(())
    }

    /// Builds a context from state and the controller's event metadata.
    fn snapshot(&self) -> RetryContext {
        self.decorate(self.state.context(self.current_attempt))
    }

    /// Attaches timeout and retry-scheduling metadata to a state context.
    fn decorate(&self, context: RetryContext) -> RetryContext {
        let context = context
            .with_attempt_timeout(self.current_attempt_timeout)
            .with_retry_after_hint(self.retry_after_hint);
        self.next_delay
            .map_or(context, |delay| context.with_next_delay(delay))
    }

    /// Constructs an aborted terminal error and consumes the last failure.
    fn aborted(&mut self) -> RetryError<E> {
        let last_failure = self
            .last_failure
            .take()
            .expect("an abort decision always follows an attempt failure");
        self.clear_current_attempt();
        RetryError::new(RetryFailure::Aborted { last_failure }, self.snapshot())
    }

    /// Constructs an exhausted terminal error from the current snapshot.
    fn exhausted(&mut self, limit: RetryLimitKind) -> RetryError<E> {
        self.clear_current_attempt();
        RetryError::new(
            RetryFailure::Exhausted {
                limit,
                last_failure: self.last_failure.take(),
            },
            self.snapshot(),
        )
    }

    /// Constructs a timeout terminal error from the current snapshot.
    fn timed_out(&mut self, scope: RetryTimeoutScope) -> RetryError<E> {
        self.clear_current_attempt();
        RetryError::new(
            RetryFailure::TimedOut {
                scope,
                last_failure: self.last_failure.take(),
            },
            self.snapshot(),
        )
    }

    /// Constructs a cancellation terminal error from the current snapshot.
    fn cancelled(&mut self, phase: RetryCancellationPhase) -> RetryError<E> {
        self.clear_current_attempt();
        self.cancelled_with_context(phase, self.snapshot())
    }

    /// Constructs cancellation from an exact context without changing its
    /// active-attempt overlay.
    fn cancelled_with_context(
        &mut self,
        phase: RetryCancellationPhase,
        context: RetryContext,
    ) -> RetryError<E> {
        RetryError::new(
            RetryFailure::Cancelled {
                phase,
                last_failure: self.last_failure.take(),
            },
            context,
        )
    }

    /// Constructs a callback terminal error from its exact callback context.
    fn callback_failed(
        &mut self,
        callback: crate::RetryCallbackFailure,
        context: RetryContext,
    ) -> RetryError<E> {
        RetryError::new(
            RetryFailure::CallbackFailed {
                callback,
                last_failure: self.last_failure.take(),
            },
            context,
        )
    }

    /// Constructs a callback failure after a best-effort elapsed-time refresh.
    ///
    /// A callback panic remains the primary terminal cause even when its
    /// post-panic clock sample is invalid. In that case, `fallback_context` is
    /// the last coherent snapshot retained by the controller.
    fn callback_failed_after_refresh(
        &mut self,
        callback: crate::RetryCallbackFailure,
        fallback_context: RetryContext,
        clock: &dyn MonotonicClock,
    ) -> RetryError<E> {
        let context = match self.state.refresh(clock.now()) {
            Ok(()) => self.snapshot(),
            Err(_) => fallback_context,
        };
        self.callback_failed(callback, context)
    }

    /// Refreshes elapsed time without replacing an already-selected terminal
    /// cause when the new clock sample is invalid.
    fn refresh_best_effort(&mut self, clock: &dyn MonotonicClock) {
        let _ = self.state.refresh(clock.now());
    }

    /// Constructs an infrastructure terminal error from its exact context.
    fn infrastructure(
        &mut self,
        failure: RetryInfrastructureFailure,
        context: RetryContext,
    ) -> RetryError<E> {
        RetryError::new(
            RetryFailure::Infrastructure {
                failure,
                last_failure: self.last_failure.take(),
            },
            context,
        )
    }

    /// Clears the event overlay after callback processing has completed.
    fn clear_current_attempt(&mut self) {
        self.current_attempt = None;
        self.current_attempt_timeout = None;
    }

    /// Converts an invalid clock sample into an inactive terminal failure.
    fn inactive_clock_failure(
        &mut self,
        error: qubit_clock::TimeError,
    ) -> RetryError<E> {
        self.clear_current_attempt();
        self.infrastructure(
            RetryInfrastructureFailure::Clock {
                message: error.to_string().into_boxed_str(),
            },
            self.snapshot(),
        )
    }
}
