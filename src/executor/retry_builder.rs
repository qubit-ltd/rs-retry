// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder for immutable retry definitions.

use std::sync::Arc;

use super::retry::Retry;
use crate::RetryFallback;
use crate::RetryPolicy;
use crate::observer::RetryObserver;
use crate::observer::RetryObservers;
use crate::rule::RetryRule;
use crate::rule::RetryRules;

/// Builds a [`Retry`] from a policy, ordered rules, and observers.
/// # Type Parameters
/// - `E`: Application error classified by registered rules and observers.
#[must_use]
pub struct RetryBuilder<E> {
    /// Validated limits and backoff shared by executions.
    policy: RetryPolicy,
    /// Action used when no registered rule classifies an application failure.
    fallback: RetryFallback,
    /// Ordered decision callbacks shared across executions.
    rules: Vec<Arc<dyn RetryRule<E>>>,
    /// Ordered lifecycle callbacks shared across executions.
    observers: Vec<Arc<dyn RetryObserver<E>>>,
}

impl<E: 'static> RetryBuilder<E> {
    /// Creates a builder from a validated policy.
    ///
    /// # Parameters
    /// - `policy`: Validated policy shared by future executions.
    ///
    /// # Returns
    /// A builder with empty callback collections.
    #[inline(always)]
    pub(crate) fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            fallback: RetryFallback::default(),
            rules: Vec::new(),
            observers: Vec::new(),
        }
    }

    /// Appends a rule. Rules are evaluated in registration order; the first
    /// non-`UseDefault` decision wins.
    ///
    /// # Type Parameters
    /// - `R`: Thread-safe callback stored for repeated execution.
    ///
    /// # Parameters
    /// - `rule`: Callback appended after previously registered callbacks.
    ///
    /// # Returns
    /// This builder retaining ownership of the callback.
    #[inline(always)]
    pub fn rule<R>(mut self, rule: R) -> Self
    where
        R: RetryRule<E>,
    {
        self.rules.push(Arc::new(rule));
        self
    }

    /// Sets the action for an application error left unclassified by rules.
    #[inline(always)]
    pub fn fallback(mut self, fallback: RetryFallback) -> Self {
        self.fallback = fallback;
        self
    }

    /// Appends an already shared rule without wrapping it in another `Arc`.
    #[inline(always)]
    pub fn shared_rule(mut self, rule: Arc<dyn RetryRule<E>>) -> Self {
        self.rules.push(rule);
        self
    }

    /// Appends an already shared observer without wrapping it in another `Arc`.
    #[inline(always)]
    pub fn shared_observer(mut self, observer: Arc<dyn RetryObserver<E>>) -> Self {
        self.observers.push(observer);
        self
    }

    /// Appends an observer. A control callback panic terminates execution with
    /// `RetryErrorReason::CallbackFailed`; completion callback panics are
    /// retained as diagnostics without changing the frozen result.
    ///
    /// # Type Parameters
    /// - `O`: Thread-safe callback stored for repeated execution.
    ///
    /// # Parameters
    /// - `observer`: Callback appended after previously registered callbacks.
    ///
    /// # Returns
    /// This builder retaining ownership of the callback.
    #[inline(always)]
    pub fn observer<O>(mut self, observer: O) -> Self
    where
        O: RetryObserver<E>,
    {
        self.observers.push(Arc::new(observer));
        self
    }

    /// Finishes the immutable retry definition.
    ///
    /// # Returns
    /// An immutable definition; each run creates fresh flow state.
    #[must_use = "run or retain the configured retry definition"]
    #[inline(always)]
    pub fn build(self) -> Retry<E> {
        Retry::new(
            self.policy,
            self.fallback,
            RetryRules::from_vec(self.rules),
            RetryObservers::from_vec(self.observers),
        )
    }
}
