// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent retry-flow decisions shared by executor facades.

#![cfg_attr(not(any(feature = "tokio", feature = "worker")), allow(dead_code))]

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;

use super::super::Retry;
use super::PreparedAttemptPlan;
use super::PreparedBackoffPlan;
use super::RetryFlowState;
use crate::AttemptFailure;
use crate::RetryCallbackFailure;
use crate::RetryCancellationPhase;
use crate::RetryCancellationToken;
use crate::RetryContext;
use crate::RetryDecision;
use crate::RetryError;
use crate::RetryErrorReason;
use crate::RetryFallback;
use crate::RetryInfrastructureFailure;
use crate::RetryLimitKind;
use crate::RetryRandomSource;
use crate::RetryTimeoutScope;
use crate::observer::RetryObservers;
use crate::rule::RetryRules;

/// Owns all runtime-independent decisions and terminal error construction.
/// # Type Parameters
/// - `'a`: Lifetime of the immutable retry definition.
/// - `E`: Owned application error retained until success or terminal
///   conversion.
pub(crate) struct RetryFlowController<'a, E> {
    /// Attempt, elapsed-budget, and backoff state.
    state: RetryFlowState<'a>,
    /// Ordered retry rules.
    rules: &'a RetryRules<E>,
    /// Ordered retry observers.
    observers: &'a RetryObservers<E>,
    /// Action used when all retry rules delegate the failure.
    fallback: RetryFallback,
    /// Last failed attempt retained until success or terminal failure.
    last_failure: Option<AttemptFailure<E>>,
    /// Hard timeout applied to each admitted attempt, when configured.
    attempt_timeout: Option<Duration>,
    /// Current attempt ordinal retained for coherent terminal contexts.
    current_attempt: Option<NonZeroU32>,
    /// Effective timeout attached to the current attempt context.
    current_hard_attempt_timeout: Option<Duration>,
    /// Delay selected by the most recent retry decision.
    next_delay: Option<Duration>,
    /// Retry-after hint selected by the most recent retry decision.
    retry_after_hint: Option<Duration>,
}

impl<'a, E: 'static> RetryFlowController<'a, E> {
    /// Creates a controller from one immutable retry definition and clock
    /// sample.
    ///
    /// # Parameters
    /// - `started_at`: Initial coherent monotonic sample.
    /// - `retry`: Borrowed policy and callback definition.
    /// - `random_source`: Sampler for uniform delays and jitter shared with
    ///   backoff state.
    /// - `attempt_timeout`: Optional hard per-attempt duration.
    /// - `flow_timeout`: Optional hard whole-flow duration.
    ///
    /// # Returns
    /// A controller with no admitted attempts or retained failures.
    #[inline]
    #[must_use = "use the prepared value or inspect the result"]
    pub(crate) fn new(
        started_at: MonotonicInstant,
        retry: &'a Retry<E>,
        random_source: Option<Arc<dyn RetryRandomSource>>,
        attempt_timeout: Option<Duration>,
        flow_timeout: Option<Duration>,
    ) -> Self {
        Self {
            state: RetryFlowState::new(started_at, retry.policy(), random_source, flow_timeout),
            rules: retry.rules(),
            observers: retry.observers(),
            fallback: retry.fallback(),
            last_failure: None,
            attempt_timeout,
            current_attempt: None,
            current_hard_attempt_timeout: None,
            next_delay: None,
            retry_after_hint: None,
        }
    }

    /// Checks all pre-attempt gates and invokes the before-attempt observers.
    ///
    /// # Errors
    /// Returns a terminal retry error when a timeout, cancellation,
    /// continuation limit, clock failure, or before-attempt observer
    /// failure stops the flow. The upcoming operation is not counted until
    /// a facade commits it after its runtime-specific preparation succeeds.
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled at the control boundary.
    /// - `cancellation`: Optional shared flow cancellation source.
    ///
    /// # Returns
    /// The post-callback sample for immutable timeout preparation.
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
        self.current_hard_attempt_timeout = None;
        self.next_delay = None;
        self.retry_after_hint = None;
        let started_context = self.snapshot();
        if let Err(callback) = self.observers.try_before_attempt(&started_context) {
            return Err(self.callback_failed_after_refresh(callback, started_context, clock));
        }
        let admission_sample =
            self.refresh_after_control_callback(clock, cancellation, RetryCancellationPhase::BeforeAttempt)?;
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

    /// Prepares one timed attempt and fixes its absolute timeout deadline.
    ///
    /// # Errors
    /// Returns a terminal retry error when the admission sample observes an
    /// invalid clock arithmetic. Admission gates run before this preparation;
    /// runtime preparation is checked again before committing the attempt.
    ///
    /// # Parameters
    /// - `admission_sample`: Coherent sample returned by before-attempt
    ///   processing.
    ///
    /// # Returns
    /// A plan whose deadline does not move during timer registration.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    #[inline]
    pub(crate) fn prepare_attempt(
        &mut self,
        admission_sample: MonotonicInstant,
    ) -> Result<PreparedAttemptPlan, RetryError<E>> {
        let timeout = match self.prepare_timeout(admission_sample) {
            Ok(timeout) => timeout,
            Err(error) => return Err(self.inactive_clock_failure(error)),
        };
        let plan = PreparedAttemptPlan::from_timeout(timeout);
        self.current_hard_attempt_timeout = plan.duration();
        Ok(plan)
    }

    /// Commits an admitted attempt after runtime preparation has succeeded.
    ///
    /// # Errors
    /// Returns a terminal retry error when a post-preparation clock sample,
    /// timeout, cancellation, or continuation limit prevents the operation
    /// from starting.
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled at the control boundary.
    /// - `cancellation`: Optional shared flow cancellation source.
    ///
    /// # Returns
    /// Unit after incrementing the admitted attempt count.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn commit_attempt(
        &mut self,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<(), RetryError<E>> {
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
        Ok(())
    }

    /// Commits a timed attempt after its absolute timer was registered.
    ///
    /// # Errors
    /// Returns a terminal retry error when registration consumed the prepared
    /// deadline or a post-registration clock, cancellation, or limit gate
    /// prevents the operation from starting.
    ///
    /// # Parameters
    /// - `plan`: Immutable deadline registered before this commit.
    /// - `clock`: Flow clock sampled at the control boundary.
    /// - `cancellation`: Optional shared flow cancellation source.
    ///
    /// # Returns
    /// Unit after admitting the prepared attempt.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn commit_prepared_attempt(
        &mut self,
        plan: PreparedAttemptPlan,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<(), RetryError<E>> {
        let now = clock.now();
        self.refresh_or_error(now)?;
        if let Some((deadline, scope)) = plan.deadline_and_scope() {
            let deadline_reached = match Self::deadline_reached(now, deadline) {
                Ok(deadline_reached) => deadline_reached,
                Err(error) => {
                    return Err(self.inactive_clock_failure(error));
                }
            };
            if deadline_reached {
                return Err(self.timed_out(scope));
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
    ///
    /// # Parameters
    /// - `failure`: Owned failure of the admitted operation.
    /// - `clock`: Flow clock sampled at the control boundary.
    /// - `cancellation`: Optional shared flow cancellation source.
    ///
    /// # Returns
    /// The selected sleep directive when another attempt remains possible.
    ///
    /// # Panics
    /// Panics only if the retained failure disappears before callback
    /// processing, violating controller ownership invariants; custom
    /// random-source panics propagate.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    pub(crate) fn record_failure(
        &mut self,
        failure: AttemptFailure<E>,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
    ) -> Result<PreparedBackoffPlan, RetryError<E>> {
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
        if let Err(callback) = self.observers.try_attempt_failed(failure, &failed_context) {
            return Err(self.callback_failed_after_refresh(callback, failed_context, clock));
        }
        let _ = self.refresh_after_control_callback(clock, cancellation, RetryCancellationPhase::Backoff)?;

        let rule_context = self.snapshot();
        let failure = self
            .last_failure
            .as_ref()
            .expect("recorded failure must remain available to callbacks");
        let decision = match self.rules.try_decide(failure, &rule_context) {
            Ok(decision) => decision,
            Err(callback) => {
                return Err(self.callback_failed_after_refresh(callback, rule_context, clock));
            }
        };
        let _ = self.refresh_after_control_callback(clock, cancellation, RetryCancellationPhase::Backoff)?;
        let failure = self
            .last_failure
            .as_ref()
            .expect("recorded failure must remain available to callbacks");
        let default_timeout = if matches!(decision, RetryDecision::UseDefault) {
            failure.timeout_scope()
        } else {
            None
        };
        let default_panic = matches!(decision, RetryDecision::UseDefault) && failure.panic().is_some();
        if let Some(scope) = default_timeout {
            return Err(self.timed_out(scope));
        }
        if matches!(decision, RetryDecision::Abort)
            || default_panic
            || (matches!(decision, RetryDecision::UseDefault) && matches!(self.fallback, RetryFallback::Abort))
        {
            return Err(self.aborted());
        }
        let decision = if matches!(decision, RetryDecision::UseDefault) {
            RetryDecision::Retry
        } else {
            decision
        };
        self.retry_after_hint = decision.retry_after_hint();
        let backoff = self.state.next_backoff(decision);
        self.next_delay = Some(backoff.effective_delay());
        self.refresh_or_error(clock.now())?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if let Some(limit) = self.state.retry_limit(backoff.effective_delay()) {
            return Err(self.exhausted(limit));
        }
        let scheduled_context = self.snapshot();
        if let Err(callback) = self.observers.try_retry_scheduled(&backoff, &scheduled_context) {
            return Err(self.callback_failed_after_refresh(callback, scheduled_context, clock));
        }
        self.clear_current_attempt();
        let _ = self.refresh_after_control_callback(clock, cancellation, RetryCancellationPhase::Backoff)?;
        if self.state.flow_timed_out() {
            return Err(self.timed_out(RetryTimeoutScope::Flow));
        }
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(RetryCancellationPhase::Backoff));
        }
        if let Some(limit) = self.state.retry_limit(backoff.effective_delay()) {
            return Err(self.exhausted(limit));
        }

        let deadline = self
            .state
            .backoff_deadline(clock.now(), backoff.effective_delay())
            .map_err(|error| self.inactive_clock_failure(error))?;
        Ok(PreparedBackoffPlan::new(deadline))
    }

    /// Finishes a successful operation and builds its terminal context.
    ///
    /// # Errors
    /// Returns a clock infrastructure failure when the completion sample is
    /// from another domain or precedes the flow or attempt start.
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled at the control boundary.
    ///
    /// # Returns
    /// The frozen success context with inactive attempt overlay cleared.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    #[inline]
    pub(crate) fn finish_success(&mut self, clock: &dyn MonotonicClock) -> Result<RetryContext, RetryError<E>> {
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
    ///
    /// # Parameters
    /// - `failure`: Runtime failure to retain when clock accounting succeeds.
    /// - `now`: Best available monotonic sample.
    ///
    /// # Returns
    /// An owned terminal error; invalid clock accounting takes precedence over
    /// the supplied failure.
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
    ///
    /// # Parameters
    /// - `failure`: Runtime failure to retain when clock accounting succeeds.
    /// - `now`: Best available monotonic sample.
    ///
    /// # Returns
    /// An owned terminal error; invalid clock accounting takes precedence over
    /// the supplied failure.
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
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled at the control boundary.
    ///
    /// # Returns
    /// A cancellation error with coherent context, or a clock infrastructure
    /// failure.
    #[inline]
    pub(crate) fn record_attempt_cancellation(&mut self, clock: &dyn MonotonicClock) -> RetryError<E> {
        if let Err(error) = self.state.finish_attempt(clock.now()) {
            return self.inactive_clock_failure(error);
        }
        self.cancelled_with_context(RetryCancellationPhase::Attempt, self.snapshot())
    }

    /// Records cancellation while no operation is active during backoff.
    ///
    /// The terminal context retains the last attempt failure and scheduling
    /// metadata. If the clock cannot be refreshed coherently, a clock
    /// infrastructure failure is returned instead.
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled at the control boundary.
    ///
    /// # Returns
    /// A cancellation error with coherent context, or a clock infrastructure
    /// failure.
    #[inline]
    pub(crate) fn record_backoff_cancellation(&mut self, clock: &dyn MonotonicClock) -> RetryError<E> {
        if let Err(error) = self.state.refresh(clock.now()) {
            return self.inactive_clock_failure(error);
        }
        self.cancelled_with_context(RetryCancellationPhase::Backoff, self.snapshot())
    }

    /// Builds a context from state and the controller's event metadata.
    ///
    /// # Returns
    /// A copy of coherent timing state with current event overlays.
    #[inline(always)]
    #[must_use = "use the prepared value or inspect the result"]
    fn snapshot(&self) -> RetryContext {
        self.decorate(self.state.context(self.current_attempt))
    }

    /// Returns whether the optional cancellation token has been cancelled.
    ///
    /// # Parameters
    /// - `cancellation`: Optional shared flow cancellation source.
    ///
    /// # Returns
    /// True only for a supplied, cancelled token.
    #[inline(always)]
    #[must_use]
    fn is_cancelled(cancellation: Option<&RetryCancellationToken>) -> bool {
        cancellation.is_some_and(RetryCancellationToken::is_cancelled)
    }

    /// Selects an absolute timeout from the current admission sample.
    ///
    /// # Parameters
    /// - `now`: Admission sample for per-attempt deadline arithmetic.
    ///
    /// # Returns
    /// Some deadline, effective duration, and scope; None without an enabled
    /// hard timeout.
    ///
    /// # Errors
    /// Returns clock-domain arithmetic or duration overflow errors.
    ///
    /// # Panics
    /// Panics only if a flow-scoped selection has no configured flow deadline.
    fn prepare_timeout(
        &self,
        now: MonotonicInstant,
    ) -> Result<Option<(MonotonicInstant, Duration, RetryTimeoutScope)>, TimeError> {
        let Some(timeout) = self.state.effective_timeout(self.attempt_timeout) else {
            return Ok(None);
        };
        let deadline = match timeout.scope() {
            RetryTimeoutScope::Attempt => now.checked_add(timeout.duration())?,
            RetryTimeoutScope::Flow => self
                .state
                .flow_deadline()?
                .expect("a flow-scoped timeout requires a configured deadline"),
        };
        Ok(Some((deadline, timeout.duration(), timeout.scope())))
    }

    /// Returns whether `now` is at or beyond a prepared same-domain deadline.
    ///
    /// # Parameters
    /// - `now`: Sample to compare.
    /// - `deadline`: Prepared absolute deadline.
    ///
    /// # Returns
    /// True at or beyond the deadline, false before it.
    ///
    /// # Errors
    /// Returns a clock-domain mismatch error.
    #[inline]
    fn deadline_reached(now: MonotonicInstant, deadline: MonotonicInstant) -> Result<bool, TimeError> {
        deadline.validate_domain(now.domain())?;
        Ok(now.elapsed_since_origin() >= deadline.elapsed_since_origin())
    }

    /// Refreshes total elapsed time or returns a structured clock failure.
    ///
    /// # Parameters
    /// - `now`: Sample used for total elapsed accounting.
    ///
    /// # Returns
    /// Unit after successful refresh.
    ///
    /// # Errors
    /// Returns an inactive clock terminal failure when accounting rejects the
    /// sample.
    #[allow(
        clippy::result_large_err,
        reason = "the controller constructs the lossless public terminal error"
    )]
    #[inline]
    fn refresh_or_error(&mut self, now: MonotonicInstant) -> Result<(), RetryError<E>> {
        if let Err(error) = self.state.refresh(now) {
            return Err(self.inactive_clock_failure(error));
        }
        Ok(())
    }

    /// Samples completed control work before observing its cancellation.
    ///
    /// # Parameters
    /// - `clock`: Flow clock sampled exactly once at this boundary.
    /// - `cancellation`: Optional shared cancellation request.
    /// - `phase`: Cancellation attribution when the request is observed.
    ///
    /// # Returns
    /// The coherent sample, reusable for timed-attempt preparation.
    ///
    /// # Errors
    /// Clock failure takes precedence over cancellation after a normally
    /// returned callback. Callback panics use their separate best-effort path.
    #[allow(
        clippy::result_large_err,
        reason = "the controller retains lossless terminal context"
    )]
    fn refresh_after_control_callback(
        &mut self,
        clock: &dyn MonotonicClock,
        cancellation: Option<&RetryCancellationToken>,
        phase: RetryCancellationPhase,
    ) -> Result<MonotonicInstant, RetryError<E>> {
        let now = clock.now();
        self.refresh_or_error(now)?;
        if Self::is_cancelled(cancellation) {
            return Err(self.cancelled(phase));
        }
        Ok(now)
    }

    /// Attaches timeout and retry-scheduling metadata to a state context.
    ///
    /// # Parameters
    /// - `context`: Coherent timing snapshot to decorate.
    ///
    /// # Returns
    /// The supplied snapshot with current timeout, hint, and selected delay.
    #[inline]
    fn decorate(&self, context: RetryContext) -> RetryContext {
        let context = context
            .with_hard_attempt_timeout(self.current_hard_attempt_timeout)
            .with_retry_after_hint(self.retry_after_hint);
        self.next_delay.map_or(context, |delay| context.with_next_delay(delay))
    }

    /// Constructs an aborted terminal error and consumes the last failure.
    ///
    /// # Returns
    /// An Abort failure owning the last operation failure and inactive context.
    ///
    /// # Panics
    /// Panics if called without a recorded attempt failure, violating the Abort
    /// invariant.
    #[inline]
    fn aborted(&mut self) -> RetryError<E> {
        let last_failure = self
            .last_failure
            .take()
            .expect("an abort decision always follows an attempt failure");
        self.clear_current_attempt();
        RetryError::new(RetryErrorReason::Aborted, Some(last_failure), self.snapshot())
    }

    /// Constructs an exhausted terminal error from the current snapshot.
    ///
    /// # Parameters
    /// - `limit`: First exhausted continuation budget.
    ///
    /// # Returns
    /// An owned terminal error after clearing the inactive attempt overlay.
    #[inline]
    fn exhausted(&mut self, limit: RetryLimitKind) -> RetryError<E> {
        self.clear_current_attempt();
        RetryError::new(
            RetryErrorReason::Exhausted { limit },
            self.last_failure.take(),
            self.snapshot(),
        )
    }

    /// Constructs a timeout terminal error from the current snapshot.
    ///
    /// # Parameters
    /// - `scope`: Hard timeout boundary responsible for stopping.
    ///
    /// # Returns
    /// An owned terminal error after clearing the inactive attempt overlay.
    #[inline]
    fn timed_out(&mut self, scope: RetryTimeoutScope) -> RetryError<E> {
        self.clear_current_attempt();
        RetryError::new(
            RetryErrorReason::TimedOut { scope },
            self.last_failure.take(),
            self.snapshot(),
        )
    }

    /// Constructs a cancellation terminal error from the current snapshot.
    ///
    /// # Parameters
    /// - `phase`: Execution phase where cancellation was observed.
    ///
    /// # Returns
    /// An owned terminal error after clearing the inactive attempt overlay.
    #[inline]
    fn cancelled(&mut self, phase: RetryCancellationPhase) -> RetryError<E> {
        self.clear_current_attempt();
        self.cancelled_with_context(phase, self.snapshot())
    }

    /// Constructs cancellation from an exact context without changing its
    /// active-attempt overlay.
    ///
    /// # Parameters
    /// - `phase`: Phase where cancellation won.
    /// - `context`: Exact snapshot, including any active-attempt overlay.
    ///
    /// # Returns
    /// An owned cancellation error retaining the last failed attempt.
    #[inline]
    fn cancelled_with_context(&mut self, phase: RetryCancellationPhase, context: RetryContext) -> RetryError<E> {
        RetryError::new(RetryErrorReason::Cancelled { phase }, self.last_failure.take(), context)
    }

    /// Constructs a callback terminal error from its exact callback context.
    ///
    /// # Parameters
    /// - `callback`: Structured panic diagnostic.
    /// - `context`: Exact callback snapshot.
    ///
    /// # Returns
    /// A terminal callback failure owning any last attempt failure.
    #[inline]
    fn callback_failed(&mut self, callback: RetryCallbackFailure, context: RetryContext) -> RetryError<E> {
        RetryError::new(
            RetryErrorReason::CallbackFailed { callback },
            self.last_failure.take(),
            context,
        )
    }

    /// Constructs a callback failure after a best-effort elapsed-time refresh.
    ///
    /// A callback panic remains the primary terminal cause even when its
    /// post-panic clock sample is invalid. In that case, `fallback_context` is
    /// the last coherent snapshot retained by the controller.
    ///
    /// # Parameters
    /// - `callback`: Structured callback panic.
    /// - `fallback_context`: Last coherent context before the callback.
    /// - `clock`: Flow clock sampled at the control boundary.
    ///
    /// # Returns
    /// A callback-primary terminal error with refreshed or fallback timing.
    #[inline]
    fn callback_failed_after_refresh(
        &mut self,
        callback: RetryCallbackFailure,
        fallback_context: RetryContext,
        clock: &dyn MonotonicClock,
    ) -> RetryError<E> {
        let context = match self.state.refresh(clock.now()) {
            Ok(()) => self.snapshot(),
            Err(_) => fallback_context,
        };
        self.callback_failed(callback, context)
    }

    /// Constructs an infrastructure terminal error from its exact context.
    ///
    /// # Parameters
    /// - `failure`: Structured runtime failure.
    /// - `context`: Exact terminal snapshot.
    ///
    /// # Returns
    /// An owned infrastructure error retaining the last attempt failure.
    #[inline]
    fn infrastructure(&mut self, failure: RetryInfrastructureFailure, context: RetryContext) -> RetryError<E> {
        RetryError::new(
            RetryErrorReason::Infrastructure { failure },
            self.last_failure.take(),
            context,
        )
    }

    /// Clears the event overlay after callback processing has completed.
    #[inline(always)]
    fn clear_current_attempt(&mut self) {
        self.current_attempt = None;
        self.current_hard_attempt_timeout = None;
    }

    /// Converts an invalid clock sample into an inactive terminal failure.
    ///
    /// # Parameters
    /// - `error`: Clock error whose message is retained.
    ///
    /// # Returns
    /// An inactive infrastructure terminal error with the last coherent timing.
    #[inline]
    fn inactive_clock_failure(&mut self, error: TimeError) -> RetryError<E> {
        self.clear_current_attempt();
        self.infrastructure(
            RetryInfrastructureFailure::Clock {
                message: error.to_string().into_boxed_str(),
            },
            self.snapshot(),
        )
    }
}
