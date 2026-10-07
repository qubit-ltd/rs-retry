// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Owned retry execution driven one operation at a time by an external owner.

use std::num::NonZeroU32;
use std::sync::Arc;

use qubit_clock::Timer;

use super::internal::RetryFlowController;
use super::retry_session_step::RetrySessionStep;
use crate::AttemptFailure;
use crate::RetryCancellationToken;
use crate::RetryConfig;
use crate::RetryError;
use crate::RetryRandomSource;
use crate::RetryResult;
use crate::RetrySuccess;

/// Legal phases of the single outstanding operation protocol.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionPhase {
    /// No attempt has started.
    Ready,
    /// One admitted operation awaits its result.
    Active,
    /// An external scheduler owns the pending retry wait.
    Waiting,
    /// Completion observers have been notified.
    Finished,
}

/// Owns a same-thread retry flow without sleeping or registering timers.
///
/// `E` is the application error; it need not implement `Clone`. Configuration
/// and controller state are owned, so a session can be moved into a delivery
/// owner without borrowing a separate configuration. Call
/// [`Self::begin_attempt`] before each operation, then [`Self::record_result`]
/// exactly once. On [`RetrySessionStep::RetryAt`], the caller waits in the
/// supplied timer's clock domain before beginning another attempt. Cancellation
/// may be checked early by calling `begin_attempt` while waiting after
/// requesting cancellation.
///
/// Like [`crate::Retry`], this facade cannot interrupt a running operation and
/// has no hard timeout API. Configured elapsed budgets are admission limits.
/// Dropping an unfinished session does not synthesize completion callbacks.
#[must_use]
pub struct RetrySession<E> {
    /// Immutable configuration retained for completion notification.
    config: RetryConfig<E>,
    /// Shared decision engine owning its immutable policy and callbacks.
    controller: RetryFlowController<E>,
    /// Stable clock domain used for control boundaries and retry deadlines.
    timer: Arc<dyn Timer>,
    /// Optional external cancellation request.
    cancellation_token: Option<RetryCancellationToken>,
    /// Prevents duplicate admission, recording, and completion callbacks.
    phase: SessionPhase,
}

impl<E: 'static> RetrySession<E> {
    /// Creates a fresh flow owning `config` and sampling `timer` for its
    /// origin.
    ///
    /// The timer supplies only the clock; this session never registers waits.
    /// Custom clock panics propagate. Callbacks do not run until admission.
    #[inline]
    pub fn new(config: RetryConfig<E>, timer: Arc<dyn Timer>) -> Self {
        Self::new_inner(config, timer, None)
    }

    /// Creates a flow using `random` for uniform backoff and jitter samples.
    ///
    /// `config` and `timer` have the same ownership and clock semantics as
    /// [`Self::new`]. This constructor supports reproducible scheduling without
    /// changing the shared retry policy. Custom clock panics propagate.
    #[inline]
    pub fn new_with_random_source(
        config: RetryConfig<E>,
        timer: Arc<dyn Timer>,
        random: Arc<dyn RetryRandomSource>,
    ) -> Self {
        Self::new_inner(config, timer, Some(random))
    }

    /// Initializes owned state from the supplied runtime resources.
    fn new_inner(config: RetryConfig<E>, timer: Arc<dyn Timer>, random: Option<Arc<dyn RetryRandomSource>>) -> Self {
        let controller = RetryFlowController::new(timer.clock().now(), &config, random, None, None);
        Self {
            config,
            controller,
            timer,
            cancellation_token: None,
            phase: SessionPhase::Ready,
        }
    }

    /// Observes `token` at admission and failure boundaries, returning this
    /// flow. Cancellation does not interrupt an operation already admitted.
    #[inline]
    pub fn with_cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// Rechecks admission, runs before-attempt observers, and returns an
    /// ordinal.
    ///
    /// The caller must wait until the last `RetryAt` before invoking this
    /// method, unless requesting cancellation during that wait. No
    /// operation is run here. Cancellation while waiting retains the same
    /// `Backoff` attribution as [`crate::Retry`]. A previously returned
    /// retry deadline never bypasses cancellation, clock validation,
    /// elapsed budgets, or the attempt limit.
    ///
    /// # Errors
    /// Returns a terminal retry error for rejected admission, invalid clocks,
    /// cancellation, or a panicking control observer. Completion observers run
    /// once and their diagnostics are attached to the error.
    ///
    /// # Panics
    /// Panics if an operation is already active or the session has finished.
    /// Custom clock panics propagate.
    #[allow(clippy::result_large_err, reason = "preserves lossless terminal retry context")]
    pub fn begin_attempt(&mut self) -> Result<NonZeroU32, RetryError<E>> {
        assert!(
            matches!(self.phase, SessionPhase::Ready | SessionPhase::Waiting),
            "begin_attempt requires a ready or waiting session"
        );
        let clock = self.timer.clock();
        let cancellation = self.cancellation_token.as_ref();
        let result =
            if self.phase == SessionPhase::Waiting && cancellation.is_some_and(RetryCancellationToken::is_cancelled) {
                Err(self.controller.record_backoff_cancellation(clock))
            } else {
                self.controller
                    .before_attempt(clock, cancellation)
                    .and_then(|_| self.controller.commit_attempt(clock, cancellation))
            };
        match result {
            Ok(()) => {
                self.phase = SessionPhase::Active;
                Ok(NonZeroU32::new(self.controller.attempts()).expect("a committed attempt has a nonzero ordinal"))
            }
            Err(error) => {
                self.phase = SessionPhase::Finished;
                self.config
                    .complete::<NonZeroU32>(Err(error))
                    .map(RetrySuccess::into_value_discarding_diagnostics)
            }
        }
    }

    /// Records `result` from the active operation and selects the next action.
    ///
    /// Failure observers and retry rules run synchronously through the shared
    /// controller. `RetryAt` contains its absolute deadline in
    /// `timer.clock()`'s domain; this method never sleeps, waits, or
    /// registers a timer. Terminal results notify completion observers once
    /// and retain their diagnostics. `T` is the owned successful value
    /// returned unchanged on completion.
    ///
    /// # Panics
    /// Panics without a matching successful `begin_attempt`, or after
    /// completion. Custom random-source and clock panics propagate.
    pub fn record_result<T>(&mut self, result: Result<T, E>) -> RetrySessionStep<T, E> {
        assert!(
            self.phase == SessionPhase::Active,
            "record_result requires an active attempt"
        );
        let clock = self.timer.clock();
        let terminal: RetryResult<T, E> = match result {
            Ok(value) => self
                .controller
                .finish_success(clock)
                .map(|context| RetrySuccess::new(value, context)),
            Err(error) => match self.controller.record_failure(
                AttemptFailure::Error(error),
                clock,
                self.cancellation_token.as_ref(),
            ) {
                Ok(plan) => {
                    self.phase = SessionPhase::Waiting;
                    return RetrySessionStep::RetryAt(plan.deadline());
                }
                Err(error) => Err(error),
            },
        };
        self.phase = SessionPhase::Finished;
        match self.config.complete(terminal) {
            Ok(success) => RetrySessionStep::Complete(success),
            Err(error) => RetrySessionStep::Failed(error),
        }
    }
}
