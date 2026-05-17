/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Internal retry event dispatcher.
//!
//! `RetryEvents` is the only object that knows how lifecycle functors are
//! stored and invoked. Runners and policies call semantic methods such as
//! `before_attempt` or `failure_decision`, while this dispatcher handles
//! listener ordering, optional panic isolation, and retry-after hint extraction.

use std::time::Duration;

use qubit_function::{
    BiConsumer,
    BiFunction,
    Consumer,
};

use super::{
    RetryAfterHint,
    RetryContext,
    RetryListeners,
};
use crate::{
    AttemptFailure,
    AttemptFailureDecision,
    RetryError,
};

/// Dispatches retry lifecycle events and isolates listener panics when enabled.
#[derive(Clone)]
pub(crate) struct RetryEvents<E> {
    /// Optional retry-after hint extractor.
    retry_after_hint: Option<RetryAfterHint<E>>,
    /// Whether listener panics should be isolated.
    isolate_listener_panics: bool,
    /// Lifecycle listeners.
    listeners: RetryListeners<E>,
}

impl<E> RetryEvents<E> {
    /// Creates a retry event dispatcher.
    ///
    /// # Parameters
    /// - `retry_after_hint`: Optional hint extractor.
    /// - `isolate_listener_panics`: Whether listener panics are isolated.
    /// - `listeners`: Lifecycle listeners.
    ///
    /// # Returns
    /// A retry event dispatcher.
    #[inline]
    pub(crate) fn new(
        retry_after_hint: Option<RetryAfterHint<E>>,
        isolate_listener_panics: bool,
        listeners: RetryListeners<E>,
    ) -> Self {
        Self {
            retry_after_hint,
            isolate_listener_panics,
            listeners,
        }
    }

    /// Extracts a retry-after hint from a failure.
    ///
    /// # Parameters
    /// - `failure`: Failure being handled.
    /// - `context`: Context captured after the failed attempt.
    ///
    /// # Returns
    /// The extracted delay hint, if any.
    #[inline]
    pub(crate) fn retry_after_hint(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> Option<Duration> {
        self.retry_after_hint
            .as_ref()
            .and_then(|hint| self.invoke_listener(|| hint.apply(failure, context)))
    }

    /// Resolves all failure listeners into one decision.
    ///
    /// # Parameters
    /// - `failure`: Attempt failure.
    /// - `context`: Failure context.
    ///
    /// # Returns
    /// Last non-default listener decision, or [`AttemptFailureDecision::UseDefault`].
    pub(crate) fn failure_decision(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> AttemptFailureDecision {
        let mut decision = AttemptFailureDecision::UseDefault;
        for listener in &self.listeners.failure {
            let current = self.invoke_listener(|| listener.apply(failure, context));
            if current != AttemptFailureDecision::UseDefault {
                // All listeners are invoked for observability. The last
                // concrete decision wins so later registrations can refine or
                // override earlier broad rules.
                decision = current;
            }
        }
        decision
    }

    /// Emits before-attempt listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    pub(crate) fn before_attempt(&self, context: &RetryContext) {
        for listener in &self.listeners.before_attempt {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits attempt-success listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    pub(crate) fn attempt_success(&self, context: &RetryContext) {
        for listener in &self.listeners.attempt_success {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits retry-scheduled listeners.
    ///
    /// # Parameters
    /// - `failure`: Failure that caused the retry to be scheduled.
    /// - `context`: Context carrying the selected next delay.
    pub(crate) fn retry_scheduled(&self, failure: &AttemptFailure<E>, context: &RetryContext) {
        for listener in &self.listeners.retry_scheduled {
            self.invoke_listener(|| {
                listener.accept(failure, context);
            });
        }
    }

    /// Emits terminal error listeners and returns the same error.
    ///
    /// # Parameters
    /// - `error`: Terminal retry error.
    ///
    /// # Returns
    /// The same error after listeners have been invoked.
    pub(crate) fn error(&self, error: RetryError<E>) -> RetryError<E> {
        // Returning the same error keeps runner code fluent:
        // `return Err(events.error(error))` both notifies listeners and
        // preserves the exact terminal error value.
        for listener in &self.listeners.error {
            self.invoke_listener(|| {
                listener.accept(&error, error.context());
            });
        }
        error
    }

    /// Invokes a listener and optionally isolates panics.
    ///
    /// # Parameters
    /// - `call`: Listener invocation closure.
    ///
    /// # Returns
    /// The listener return value, or `Default::default()` when an isolated panic
    /// occurs.
    fn invoke_listener<R>(&self, call: impl FnOnce() -> R) -> R
    where
        R: Default,
    {
        if self.isolate_listener_panics {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_default()
        } else {
            call()
        }
    }
}
