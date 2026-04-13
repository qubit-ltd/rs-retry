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
//! The builder collects options, an error classifier, and listeners before
//! producing a validated [`RetryExecutor`].

use std::sync::Arc;
use std::time::Duration;

use qubit_common::BoxError;
use qubit_function::{ArcBiFunction, ArcConsumer, BiPredicate};

use crate::events::RetryListeners;
use crate::{
    AbortEvent, AttemptContext, Delay, FailureEvent, Jitter, RetryConfigError, RetryDecision,
    RetryEvent, RetryOptions, SuccessEvent,
};

use crate::error::ErrorClassifier;
use crate::retry_executor::RetryExecutor;

/// Builder for [`RetryExecutor`].
///
/// The generic parameter `E` is the application error type that the resulting
/// executor will classify. If no classifier is provided, the built executor retries
/// every application error until limits stop execution.
pub struct RetryExecutorBuilder<E = BoxError> {
    /// Retry limits, delays, jitter, and other tunables accumulated by the builder.
    options: RetryOptions,
    /// Optional classifier; when absent, every application error is treated as retryable.
    classifier: Option<ErrorClassifier<E>>,
    /// Hooks invoked on success, failure, abort, and each retry attempt.
    listeners: RetryListeners<E>,
    /// Set when `max_attempts` was configured as zero; surfaced from [`Self::build`].
    max_attempts_error: Option<RetryConfigError>,
}

impl<E> RetryExecutorBuilder<E> {
    /// Creates a builder with default options and a retry-all classifier.
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
            classifier: None,
            listeners: RetryListeners::default(),
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
                RetryOptions::KEY_MAX_ATTEMPTS,
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
    /// This method does not return errors immediately. Delay validation occurs
    /// in [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn delay(mut self, delay: Delay) -> Self {
        self.options.delay = delay;
        self
    }

    /// Sets the jitter strategy.
    ///
    /// # Parameters
    /// - `jitter`: Jitter strategy to apply to each base delay.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors immediately. Jitter validation occurs
    /// in [`RetryExecutorBuilder::build`].
    #[inline]
    pub fn jitter(mut self, jitter: Jitter) -> Self {
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
        self.jitter(Jitter::Factor(factor))
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
        P: BiPredicate<E, AttemptContext> + Send + Sync + 'static,
    {
        self.classifier = Some(ArcBiFunction::new(move |error, context| {
            if retry_tester.test(error, context) {
                RetryDecision::Retry
            } else {
                RetryDecision::Abort
            }
        }));
        self
    }

    /// Uses a classifier returning [`RetryDecision`].
    ///
    /// # Parameters
    /// - `classifier`: Callback that receives the application error and
    ///   attempt context and returns a retry decision.
    ///
    /// # Returns
    /// The updated builder.
    ///
    /// # Errors
    /// This method does not return errors.
    ///
    /// # Panics
    /// The built executor propagates any panic raised by `classifier`.
    pub fn classify_error<F>(mut self, classifier: F) -> Self
    where
        F: Fn(&E, &AttemptContext) -> RetryDecision + Send + Sync + 'static,
    {
        self.classifier = Some(ArcBiFunction::new(classifier));
        self
    }

    /// Registers a listener invoked before retry sleep.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with a [`RetryEvent`] after a failed
    ///   attempt and before sleeping.
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
        F: for<'a> Fn(&RetryEvent<'a, E>) + Send + Sync + 'static,
    {
        self.listeners.retry = Some(Arc::new(listener));
        self
    }

    /// Registers a listener invoked when the operation succeeds.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with a [`SuccessEvent`] when the
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
        F: Fn(&SuccessEvent) + Send + Sync + 'static,
    {
        self.listeners.success = Some(ArcConsumer::new(listener));
        self
    }

    /// Registers a listener invoked when retry limits are exhausted.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with a [`FailureEvent`] when retry limits
    ///   stop execution.
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
        F: for<'a> Fn(&FailureEvent<'a, E>) + Send + Sync + 'static,
    {
        self.listeners.failure = Some(Arc::new(listener));
        self
    }

    /// Registers a listener invoked when the classifier aborts retrying.
    ///
    /// # Parameters
    /// - `listener`: Callback invoked with an [`AbortEvent`] when the
    ///   classifier aborts retrying.
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
        F: for<'a> Fn(&AbortEvent<'a, E>) + Send + Sync + 'static,
    {
        self.listeners.abort = Some(Arc::new(listener));
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
        // If no classifier is provided, treat all errors as retryable.
        let classifier = self
            .classifier
            .unwrap_or_else(|| ArcBiFunction::constant(RetryDecision::Retry));
        Ok(RetryExecutor::new(self.options, classifier, self.listeners))
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
