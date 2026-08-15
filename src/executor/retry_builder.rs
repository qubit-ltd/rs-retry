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
#[must_use]
pub struct RetryBuilder<E> {
    policy: RetryPolicy,
    rules: RetryRules<E>,
    observers: RetryObservers<E>,
}

impl<E: 'static> RetryBuilder<E> {
    /// Creates a builder from a validated policy.
    pub(crate) fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            rules: RetryRules::default(),
            observers: RetryObservers::default(),
        }
    }

    /// Appends a rule. Rules are evaluated in registration order; the first
    /// non-`UseDefault` decision wins.
    pub fn rule<R>(mut self, rule: R) -> Self
    where
        R: RetryRule<E>,
    {
        self.rules.push(rule);
        self
    }

    /// Appends an observer. An observer panic terminates execution with a
    /// structured `RetryFailure::CallbackFailed` value.
    pub fn observer<O>(mut self, observer: O) -> Self
    where
        O: RetryObserver<E>,
    {
        self.observers.push(observer);
        self
    }

    /// Finishes the immutable retry definition.
    #[must_use]
    pub fn build(self) -> Retry<E> {
        Retry::new(self.policy, self.rules, self.observers)
    }
}
