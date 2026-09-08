// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder for immutable retry configurations.

use std::sync::Arc;
use std::time::Duration;

use super::retry_config::RetryConfig;
use crate::RetryFallback;
use crate::RetryPolicy;
use crate::RetryPolicyBuilder;
use crate::RetryPolicyError;
use crate::backoff::BackoffPolicy;
use crate::observer::RetryObserver;
use crate::observer::RetryObservers;
use crate::rule::RetryRule;
use crate::rule::RetryRules;

/// Builds a [`RetryConfig`] from policy limits, ordered rules, and observers.
///
/// Policy builder methods are forwarded directly so one chain can configure
/// both limits and callbacks:
///
/// ```
/// use qubit_retry::RetryConfig;
/// use qubit_retry::RetryFallback;
///
/// let config = RetryConfig::<&str>::builder()
///     .max_attempts(2)
///     .fallback(RetryFallback::Retry)
///     .build()?;
/// assert_eq!(config.policy().admission_limits().max_attempts().get(), 2);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
///
/// # Type Parameters
/// - `E`: Application error classified by registered rules and observers.
#[must_use]
pub struct RetryConfigBuilder<E> {
    /// Policy builder used when no built policy override is present.
    policy_builder: RetryPolicyBuilder,
    /// Replaces `policy_builder` output when set explicitly or by `policy`.
    built_policy: Option<RetryPolicy>,
    /// Action used when no registered rule classifies an application failure.
    fallback: RetryFallback,
    /// Ordered decision callbacks shared across executions.
    rules: Vec<Arc<dyn RetryRule<E>>>,
    /// Ordered lifecycle callbacks shared across executions.
    observers: Vec<Arc<dyn RetryObserver<E>>>,
}

impl<E: 'static> RetryConfigBuilder<E> {
    /// Creates a builder with default policy limits.
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self {
            policy_builder: RetryPolicyBuilder::new(),
            built_policy: None,
            fallback: RetryFallback::default(),
            rules: Vec::new(),
            observers: Vec::new(),
        }
    }

    /// Uses a validated policy instead of the embedded policy builder state.
    ///
    /// Later policy-builder methods replace this override by rebuilding the
    /// embedded builder from the supplied policy.
    #[inline(always)]
    pub fn policy(mut self, policy: RetryPolicy) -> Self {
        self.built_policy = Some(policy);
        self
    }

    /// Sets the maximum number of attempts, including the first attempt.
    #[inline(always)]
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.max_attempts(max_attempts);
        self
    }

    /// Sets the cumulative operation-time budget.
    #[inline(always)]
    pub fn operation_time_budget(mut self, elapsed: Duration) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.operation_time_budget(elapsed);
        self
    }

    /// Sets or removes the cumulative operation-time budget.
    #[inline(always)]
    pub fn operation_time_budget_opt(mut self, elapsed: Option<Duration>) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.operation_time_budget_opt(elapsed);
        self
    }

    /// Removes the cumulative operation-time budget.
    #[inline(always)]
    pub fn without_operation_time_budget(mut self) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.without_operation_time_budget();
        self
    }

    /// Sets the whole-flow monotonic elapsed budget.
    #[inline(always)]
    pub fn total_time_budget(mut self, elapsed: Duration) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.total_time_budget(elapsed);
        self
    }

    /// Sets or removes the whole-flow budget.
    #[inline(always)]
    pub fn total_time_budget_opt(mut self, elapsed: Option<Duration>) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.total_time_budget_opt(elapsed);
        self
    }

    /// Removes the whole-flow monotonic elapsed budget.
    #[inline(always)]
    pub fn without_total_time_budget(mut self) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.without_total_time_budget();
        self
    }

    /// Sets the pure backoff policy.
    #[inline(always)]
    pub fn backoff(mut self, backoff: BackoffPolicy) -> Self {
        self.ensure_policy_builder();
        self.policy_builder = self.policy_builder.backoff(backoff);
        self
    }

    /// Appends a rule. Rules are evaluated in registration order; the first
    /// non-`UseDefault` decision wins.
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
    #[inline(always)]
    pub fn observer<O>(mut self, observer: O) -> Self
    where
        O: RetryObserver<E>,
    {
        self.observers.push(Arc::new(observer));
        self
    }

    /// Finishes the immutable retry configuration.
    ///
    /// # Returns
    /// An immutable configuration; each run creates fresh flow state.
    ///
    /// # Errors
    /// Returns a policy error when the configured limits are invalid.
    #[must_use = "run or retain the configured retry configuration"]
    #[inline]
    pub fn build(self) -> Result<RetryConfig<E>, RetryPolicyError> {
        let policy = match self.built_policy {
            Some(policy) => policy,
            None => self.policy_builder.build()?,
        };
        Ok(RetryConfig::new(
            policy,
            self.fallback,
            RetryRules::from_vec(self.rules),
            RetryObservers::from_vec(self.observers),
        ))
    }

    /// Ensures later policy-builder calls mutate an embedded builder.
    #[inline(always)]
    fn ensure_policy_builder(&mut self) {
        if let Some(policy) = self.built_policy.take() {
            self.policy_builder = RetryPolicyBuilder::from_policy(policy);
        }
    }
}
