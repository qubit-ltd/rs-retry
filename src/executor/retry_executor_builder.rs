/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry executor builder.
//!
//! The builder collects options, a retry decider, and listeners before
//! producing a validated [`RetryExecutor`].

use std::time::Duration;

use qubit_common::BoxError;
use qubit_function::{ArcBiFunction, BiFunction, BiPredicate};

use crate::constants::KEY_MAX_ATTEMPTS;
use crate::event::RetryListeners;
use crate::{
    RetryAbortContext, RetryAbortListener, RetryAttemptContext, RetryAttemptFailure,
    RetryConfigError, RetryContext, RetryDecision, RetryDelay, RetryFailureContext,
    RetryFailureListener, RetryJitter, RetryListener, RetryOptions, RetrySuccessContext,
    RetrySuccessListener,
};

use super::retry_executor::RetryExecutor;
use crate::error::RetryDecider;

/// Builder for [`RetryExecutor`].
///
/// The generic parameter `E` is the application error type that the resulting
/// executor will pass errors to. If no decider is provided, the built executor retries
/// every application error until limits stop execution.
pub struct RetryExecutorBuilder<E = BoxError> {
    /// Retry limits, delays, jitter, and other tunables accumulated by the builder.
    options: RetryOptions,
    /// Optional [`RetryDecider`]; when absent, every application error is treated as retryable.
    retry_decider: Option<RetryDecider<E>>,
    /// Decision used for [`crate::RetryAttemptFailure::AttemptTimeout`].
    timeout_decision: RetryDecision,
    /// Hooks invoked on success, failure, abort, and each retry attempt.
    listeners: RetryListeners<E>,
    /// Whether listener panics should be isolated instead of propagated.
    isolate_listener_panics: bool,
    /// Set when `max_attempts` was configured as zero; surfaced from [`Self::build`].
    max_attempts_error: Option<RetryConfigError>,
}

impl<E> RetryExecutorBuilder<E> {
    /// Creates a builder with default options and a retry-all decider.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A builder with [`RetryOptions::default`] and no listeners.
    #[inline]
    pub fn new() -> Self {
        Self {
            options: RetryOptions::default(),
            retry_decider: None,
            timeout_decision: RetryDecision::Retry,
            listeners: RetryListeners::default(),
            isolate_listener_panics: false,
            max_attempts_error: None,
        }
    }

    /// Replaces all options with an existing option snapshot.
    ///
    /// # Parameters
    /// - `options`: Retry options to install in the builder.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors immediately. Validation occurs in
    /// [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn options(mut self, options: RetryOptions) -> Self {
        self.options = options;
        self.max_attempts_error = None;
        self
    }

    /// Sets the maximum number of attempts.
    ///
    /// # Parameters
    /// - `max_attempts`: Maximum attempts, including the initial attempt.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method records a configuration error when `max_attempts` is zero.
    /// The error is returned later by [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        if let Some(max_attempts) = std::num::NonZeroU32::new(max_attempts) {
            self.options.max_attempts = max_attempts;
            self.max_attempts_error = None;
        } else {
            self.max_attempts_error = Some(RetryConfigError::invalid_value(
                KEY_MAX_ATTEMPTS,
                "max_attempts must be greater than zero",
            ));
        }
        self
    }

    /// Sets the maximum total elapsed time.
    ///
    /// # Parameters
    /// - `max_elapsed`: Optional total elapsed-time budget for the whole retry
    ///   execution. `None` means unlimited.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn max_elapsed(mut self, max_elapsed: Option<Duration>) -> Self {
        self.options.max_elapsed = max_elapsed;
        self
    }

    /// Sets the base delay strategy.
    ///
    /// # Parameters
    /// - `delay`: Base delay strategy to use between attempts.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors immediately. RetryDelay validation occurs
    /// in [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn delay(mut self, delay: RetryDelay) -> Self {
        self.options.delay = delay;
        self
    }

    /// Sets the jitter strategy.
    ///
    /// # Parameters
    /// - `jitter`: RetryJitter strategy to apply to each base delay.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors immediately. RetryJitter validation occurs
    /// in [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn jitter(mut self, jitter: RetryJitter) -> Self {
        self.options.jitter = jitter;
        self
    }

    /// Sets the jitter strategy from a relative factor.
    ///
    /// # Parameters
    /// - `factor`: Relative jitter range. Valid values are finite and within
    ///   `[0.0, 1.0]`.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors immediately. Factor validation occurs
    /// in [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn jitter_factor(self, factor: f64) -> Self {
        self.jitter(RetryJitter::Factor(factor))
    }

    /// Uses a boolean retry tester where `true` means retry.
    ///
    /// # Parameters
    /// - `retry_tester`: Predicate that receives the application error and
    ///   attempt context. Returning `true` maps to [`RetryDecision::Retry`];
    ///   returning `false` maps to [`RetryDecision::Abort`].
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `retry_tester`.
    pub fn retry_if<P>(mut self, retry_tester: P) -> Self
    where
        P: BiPredicate<E, RetryAttemptContext> + Send + Sync + 'static,
    {
        self.retry_decider = Some(ArcBiFunction::new(move |error, context| {
            if retry_tester.test(error, context) {
                RetryDecision::Retry
            } else {
                RetryDecision::Abort
            }
        }));
        self
    }

    /// Chooses [`RetryDecision`] for each failed attempt from the error and
    /// [`RetryAttemptContext`].
    ///
    /// # Parameters
    /// - `decider`: Any [`BiFunction`] over the application error and
    ///   [`RetryAttemptContext`] (including closures); it is converted with
    ///   [`BiFunction::into_arc`]. The decider itself must be `Send + Sync +
    ///   'static`; the application error type `E` is not required to be `'static`.
    ///   If type inference fails for a closure, annotate parameters (for example
    ///   `|e: &E, ctx: &RetryAttemptContext|`).
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `decider`.
    pub fn retry_decide<B>(mut self, decider: B) -> Self
    where
        B: BiFunction<E, RetryAttemptContext, RetryDecision> + Send + Sync + 'static,
    {
        self.retry_decider = Some(decider.into_arc());
        self
    }

    /// Sets how attempt timeouts should be handled.
    ///
    /// # Parameters
    /// - `decision`: Decision used when one attempt ends with
    ///   [`crate::RetryAttemptFailure::AttemptTimeout`].
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn timeout_decision(mut self, decision: RetryDecision) -> Self {
        self.timeout_decision = decision;
        self
    }

    /// Convenience helper to abort retrying when one attempt times out.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn abort_on_timeout(self) -> Self {
        self.timeout_decision(RetryDecision::Abort)
    }

    /// Registers a listener invoked before retry sleep.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with [`RetryContext`] plus the triggering
    ///   [`RetryAttemptFailure`] after a failed attempt and before sleeping.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `listener`.
    pub fn on_retry<F>(mut self, listener: F) -> Self
    where
        F: Fn(&RetryContext, &RetryAttemptFailure<E>) + Send + Sync + 'static,
    {
        self.listeners.retry.push(RetryListener::new(listener));
        self
    }

    /// Registers a listener invoked when the operation succeeds.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with a [`RetrySuccessContext`] when the
    ///   operation eventually succeeds.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `listener`.
    pub fn on_success<F>(mut self, listener: F) -> Self
    where
        F: Fn(&RetrySuccessContext) + Send + Sync + 'static,
    {
        self.listeners.success.push(RetrySuccessListener::new(listener));
        self
    }

    /// Registers a listener invoked when retry limits are exhausted.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with [`RetryFailureContext`] metadata plus
    ///   `Option<RetryAttemptFailure<E>>` when retry limits stop execution.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `listener`.
    pub fn on_failure<F>(mut self, listener: F) -> Self
    where
        F: Fn(&RetryFailureContext, &Option<RetryAttemptFailure<E>>) + Send + Sync + 'static,
    {
        self.listeners.failure.push(RetryFailureListener::new(listener));
        self
    }

    /// Registers a listener invoked when the retry decider aborts retrying.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with [`RetryAbortContext`] metadata plus the
    ///   failure when the retry decider aborts retrying.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `listener`.
    pub fn on_abort<F>(mut self, listener: F) -> Self
    where
        F: Fn(&RetryAbortContext, &RetryAttemptFailure<E>) + Send + Sync + 'static,
    {
        self.listeners.abort.push(RetryAbortListener::new(listener));
        self
    }

    /// Enables panic isolation for all registered listeners.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn isolate_listener_panics(mut self) -> Self {
        self.isolate_listener_panics = true;
        self
    }

    /// Builds and validates the executor.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// A validated [`RetryExecutor`].
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] when `max_attempts` was set to zero or when
    /// delay or jitter validation fails.
    pub fn build(self) -> Result<RetryExecutor<E>, RetryConfigError> {
        if let Some(error) = self.max_attempts_error {
            return Err(error);
        }
        self.options.validate()?;
        // If no decider is provided, treat all errors as retryable.
        let retry_decider = self
            .retry_decider
            .unwrap_or_else(|| ArcBiFunction::constant(RetryDecision::Retry));
        Ok(RetryExecutor::new(
            self.options,
            retry_decider,
            self.timeout_decision,
            self.isolate_listener_panics,
            self.listeners,
        ))
    }
}

impl<E> Default for RetryExecutorBuilder<E> {
    /// Creates a default retry executor builder.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A builder equivalent to [`RetryExecutorBuilder::new`].
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
