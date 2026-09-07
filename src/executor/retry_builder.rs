// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder for immutable retry definitions.

use super::retry::Retry;
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
    /// Ordered decision callbacks shared across executions.
    rules: RetryRules<E>,
    /// Ordered lifecycle callbacks shared across executions.
    observers: RetryObservers<E>,
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
            rules: RetryRules::default(),
            observers: RetryObservers::default(),
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
        self.rules.push(rule);
        self
    }

    /// Appends an observer. A control callback panic terminates execution with
    /// `RetryFailure::CallbackFailed`; completion callback panics are retained
    /// as diagnostics without changing the frozen result.
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
        self.observers.push(observer);
        self
    }

    /// Finishes the immutable retry definition.
    ///
    /// # Returns
    /// An immutable definition; each run creates fresh flow state.
    #[must_use = "run or retain the configured retry definition"]
    #[inline(always)]
    pub fn build(self) -> Retry<E> {
        Retry::new(self.policy, self.rules, self.observers)
    }
}
