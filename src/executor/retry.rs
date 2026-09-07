// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable retry facade.

#[cfg(feature = "tokio")]
use super::async_retry::AsyncRetry;
use super::retry_builder::RetryBuilder;
use super::sync_retry::SyncRetry;
#[cfg(feature = "worker")]
use super::worker_retry::WorkerRetry;
use crate::RetryFallback;
use crate::RetryPolicy;
use crate::RetryResult;
use crate::observer::RetryObservers;
use crate::rule::RetryRules;

/// Immutable retry definition bound to an operation error type.
///
/// A [`Retry`] contains only pure policy data and ordered callbacks. Runtime
/// resources such as clocks, timers, and random sources belong to the selected
/// execution facade, so cloning a retry definition is cheap and deterministic.
///
/// # Type Parameters
/// - `E`: Application error passed by reference to shared rules and observers.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryFallback;
/// use qubit_retry::RetryPolicy;
///
/// let retry = Retry::<&str>::builder(RetryPolicy::builder().max_attempts(2).build()?)
///     .fallback(RetryFallback::Retry)
///     .build();
/// let mut calls = 0;
/// let success = retry.sync().run(|| {
///     calls += 1;
///     if calls == 1 { Err("busy") } else { Ok(42) }
/// }).expect("second attempt succeeds");
/// assert_eq!(*success.value(), 42);
/// assert_eq!(success.context().attempts(), 2);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
pub struct Retry<E> {
    /// Validated limits and backoff shared by executions.
    policy: RetryPolicy,
    fallback: RetryFallback,
    /// Ordered decision callbacks shared across executions.
    rules: RetryRules<E>,
    /// Ordered lifecycle callbacks shared across executions.
    observers: RetryObservers<E>,
}

/// Clones the immutable definition without constraining the operation error.
///
/// Callback collections are reference-counted; each execution still creates
/// fresh runtime state.
impl<E> Clone for Retry<E> {
    ///
    /// # Returns
    /// A definition sharing callbacks while retaining the same pure policy.
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

impl<E: 'static> Retry<E> {
    /// Starts building a retry definition from a validated policy.
    ///
    /// # Parameters
    /// - `policy`: Validated policy applied to each independent execution.
    ///
    /// # Returns
    /// A builder with no custom rules or observers.
    #[inline(always)]
    #[must_use = "configure and build the retry definition"]
    pub fn builder(policy: RetryPolicy) -> RetryBuilder<E> {
        RetryBuilder::new(policy)
    }

    /// Selects same-thread execution. This mode intentionally exposes no
    /// timeout because Rust cannot safely interrupt an arbitrary closure.
    ///
    /// # Returns
    /// A facade borrowing this definition with standard timer and random
    /// source.
    #[must_use = "configure and run the selected execution facade"]
    #[inline(always)]
    pub fn sync(&self) -> SyncRetry<'_, E> {
        SyncRetry::new(self)
    }

    /// Selects Tokio execution with per-attempt and whole-flow timeouts.
    ///
    /// # Returns
    /// A facade borrowing this definition; the Tokio timer is selected when
    /// run.
    #[cfg(feature = "tokio")]
    #[must_use = "configure and run the selected execution facade"]
    #[inline(always)]
    pub fn asynchronous(&self) -> AsyncRetry<'_, E> {
        AsyncRetry::new(self)
    }

    /// Selects worker-thread execution with cooperative cancellation.
    ///
    /// # Returns
    /// A facade borrowing this definition with cooperative OS-thread cleanup.
    #[must_use = "configure and run the selected execution facade"]
    #[inline(always)]
    #[cfg(feature = "worker")]
    pub fn worker(&self) -> WorkerRetry<'_, E>
    where
        E: Send,
    {
        WorkerRetry::new(self)
    }

    ///
    /// Constructs the definition from its validated parts.
    ///
    /// # Parameters
    /// - `policy`: Immutable limits and backoff.
    /// - `rules`: Ordered decision callbacks.
    /// - `observers`: Ordered lifecycle callbacks.
    ///
    /// # Returns
    /// A definition owning the supplied configuration.
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

    pub(crate) fn fallback(&self) -> RetryFallback {
        self.fallback
    }

    ///
    /// Returns the registered callbacks.
    ///
    /// # Returns
    /// The ordered rule collection borrowed from this definition.
    #[inline(always)]
    #[must_use]
    pub(crate) fn rules(&self) -> &RetryRules<E> {
        &self.rules
    }

    ///
    /// Returns the registered callbacks.
    ///
    /// # Returns
    /// The observer collection borrowed from this definition.
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
