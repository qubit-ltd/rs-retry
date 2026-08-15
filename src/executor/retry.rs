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
use super::worker_retry::WorkerRetry;
use crate::RetryPolicy;
use crate::observer::RetryObservers;
use crate::rule::RetryRules;

/// Immutable retry definition bound to an operation error type.
///
/// A [`Retry`] contains only pure policy data and ordered callbacks. Runtime
/// resources such as clocks, timers, and random sources belong to the selected
/// execution facade, so cloning a retry definition is cheap and deterministic.
#[derive(Clone)]
pub struct Retry<E> {
    policy: RetryPolicy,
    rules: RetryRules<E>,
    observers: RetryObservers<E>,
}

impl<E: 'static> Retry<E> {
    /// Starts building a retry definition from a validated policy.
    pub fn builder(policy: RetryPolicy) -> RetryBuilder<E> {
        RetryBuilder::new(policy)
    }

    pub(crate) fn new(
        policy: RetryPolicy,
        rules: RetryRules<E>,
        observers: RetryObservers<E>,
    ) -> Self {
        Self {
            policy,
            rules,
            observers,
        }
    }

    /// Returns the immutable retry policy.
    #[must_use = "use the policy to inspect retry configuration"]
    pub fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// Selects same-thread execution. This mode intentionally exposes no
    /// timeout because Rust cannot safely interrupt an arbitrary closure.
    #[must_use]
    pub fn sync(&self) -> SyncRetry<'_, E> {
        SyncRetry::new(self)
    }

    /// Selects Tokio execution with per-attempt and whole-flow timeouts.
    #[cfg(feature = "tokio")]
    #[must_use]
    pub fn asynchronous(&self) -> AsyncRetry<'_, E> {
        AsyncRetry::new(self)
    }

    /// Selects worker-thread execution with cooperative cancellation.
    #[must_use]
    pub fn worker(&self) -> WorkerRetry<'_, E>
    where
        E: Send,
    {
        WorkerRetry::new(self)
    }

    pub(crate) fn rules(&self) -> &RetryRules<E> {
        &self.rules
    }

    pub(crate) fn observers(&self) -> &RetryObservers<E> {
        &self.observers
    }
}
