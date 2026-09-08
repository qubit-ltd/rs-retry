// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable retry configuration.

use super::retry_config_builder::RetryConfigBuilder;
use crate::RetryFallback;
use crate::RetryPolicy;
use crate::RetryResult;
use crate::observer::RetryObservers;
use crate::rule::RetryRules;

/// Immutable retry configuration bound to an operation error type.
///
/// A [`RetryConfig`] contains only pure policy data and ordered callbacks.
/// Runtime resources such as clocks, timers, and random sources belong to the
/// selected execution facade, so cloning a configuration is cheap and
/// deterministic.
///
/// Construct an executor with [`Retry::new`](super::Retry::new).
#[cfg_attr(
    feature = "tokio",
    doc = " For async code use [`TokioRetry::new`](super::TokioRetry::new)."
)]
#[cfg_attr(
    feature = "worker",
    doc = " For blocking offload use [`WorkerRetry::new`](super::WorkerRetry::new)."
)]
///
/// # Type Parameters
/// - `E`: Application error passed by reference to shared rules and observers.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryConfig;
/// use qubit_retry::RetryFallback;
///
/// let config = RetryConfig::<&str>::builder()
///     .max_attempts(2)
///     .fallback(RetryFallback::Retry)
///     .build()?;
/// let mut calls = 0;
/// let success = Retry::new(&config).run(|| {
///     calls += 1;
///     if calls == 1 { Err("busy") } else { Ok(42) }
/// }).expect("second attempt succeeds");
/// assert_eq!(*success.value(), 42);
/// assert_eq!(success.context().attempts(), 2);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
pub struct RetryConfig<E> {
    /// Validated limits and backoff shared by executions.
    policy: RetryPolicy,
    /// Action used when all retry rules delegate an application failure.
    fallback: RetryFallback,
    /// Ordered decision callbacks shared across executions.
    rules: RetryRules<E>,
    /// Ordered lifecycle callbacks shared across executions.
    observers: RetryObservers<E>,
}

/// Clones the immutable configuration without constraining the operation error.
///
/// Callback collections are reference-counted; each execution still creates
/// fresh runtime state.
impl<E> Clone for RetryConfig<E> {
    ///
    /// # Returns
    /// A configuration sharing callbacks while retaining the same pure policy.
    #[inline(always)]
    fn clone(&self) -> Self {
        Self {
            policy: self.policy.clone(),
            fallback: self.fallback,
            rules: self.rules.clone(),
            observers: self.observers.clone(),
        }
    }
}

impl<E: 'static> RetryConfig<E> {
    /// Starts building a retry configuration with default policy limits.
    ///
    /// # Returns
    /// A builder with no custom rules or observers.
    #[inline(always)]
    #[must_use = "configure and build the retry configuration"]
    pub fn builder() -> RetryConfigBuilder<E> {
        RetryConfigBuilder::new()
    }

    ///
    /// Constructs the configuration from its validated parts.
    ///
    /// # Parameters
    /// - `policy`: Immutable limits and backoff.
    /// - `rules`: Ordered decision callbacks.
    /// - `observers`: Ordered lifecycle callbacks.
    ///
    /// # Returns
    /// A configuration owning the supplied components.
    #[inline(always)]
    pub(crate) fn new(
        policy: RetryPolicy,
        fallback: RetryFallback,
        rules: RetryRules<E>,
        observers: RetryObservers<E>,
    ) -> Self {
        Self {
            policy,
            fallback,
            rules,
            observers,
        }
    }

    /// Returns the immutable retry policy.
    ///
    /// # Returns
    /// The borrowed immutable policy.
    #[must_use = "use the policy to inspect retry configuration"]
    #[inline(always)]
    pub fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// Returns the configured fallback action for internal executors.
    #[inline(always)]
    #[must_use]
    pub(crate) fn fallback(&self) -> RetryFallback {
        self.fallback
    }

    ///
    /// Returns the registered callbacks.
    ///
    /// # Returns
    /// The ordered rule collection borrowed from this configuration.
    #[inline(always)]
    #[must_use]
    pub(crate) fn rules(&self) -> &RetryRules<E> {
        &self.rules
    }

    ///
    /// Returns the registered callbacks.
    ///
    /// # Returns
    /// The observer collection borrowed from this configuration.
    #[inline(always)]
    #[must_use]
    pub(crate) fn observers(&self) -> &RetryObservers<E> {
        &self.observers
    }

    /// Notifies completion once and attaches diagnostics to a frozen result.
    ///
    /// # Parameters
    /// - `result`: Final execution result after all runtime cleanup decisions.
    ///
    /// # Returns
    /// The original result with completion callback failures in registration
    /// order. These callbacks run synchronously and cannot change the outcome.
    /// Only returned results reach this boundary; operation unwinding and a
    /// dropped async execution do not synthesize a completion notification.
    ///
    /// # Type Parameters
    /// - `T`: Successful operation value, preserved without conversion.
    ///
    /// # Errors
    /// Returns the original terminal failure with completion diagnostics
    /// attached; completion observers do not replace or reclassify the
    /// failure.
    #[allow(clippy::result_large_err, reason = "completion preserves the lossless public result")]
    pub(super) fn complete<T>(&self, mut result: RetryResult<T, E>) -> RetryResult<T, E> {
        let diagnostics = match &result {
            Ok(success) => self.observers.notify_success(success.context()),
            Err(error) => self.observers.notify_terminal_failure(error.reason(), error.context()),
        };
        match &mut result {
            Ok(success) => success.set_completion_callback_failures(diagnostics),
            Err(error) => error.set_completion_callback_failures(diagnostics),
        }
        result
    }
}
