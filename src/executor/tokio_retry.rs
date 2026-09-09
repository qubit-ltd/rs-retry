// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tokio execution facade for the policy-based retry API.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use qubit_clock::Timer;
use qubit_clock::TokioTimer;

use super::async_retry::AsyncRetry;
use super::retry_config::RetryConfig;
use crate::RetryCancellationToken;
use crate::RetryError;
use crate::RetryRandomSource;
use crate::RetrySuccess;

/// Tokio retry execution with explicit attempt and flow timeout controls.
///
/// This facade preserves the Tokio-backed default timer while sharing the
/// runtime-independent retry implementation with [`AsyncRetry`].
#[must_use]
pub struct TokioRetry<'a, E> {
    /// Runtime-independent async facade configured for this execution.
    inner: AsyncRetry<'a, E>,
}

impl<'a, E: 'static> TokioRetry<'a, E> {
    /// Creates a Tokio retry executor from one immutable configuration.
    ///
    /// # Parameters
    /// - `config`: Configuration that must outlive this facade.
    ///
    /// # Returns
    /// A Tokio-backed async execution facade.
    #[inline]
    pub fn new(config: &'a RetryConfig<E>) -> Self {
        Self {
            inner: AsyncRetry::new(config),
        }
    }

    /// Sets the maximum duration of one admitted attempt.
    ///
    /// # Parameters
    /// - `timeout`: Hard duration for one attempt.
    ///
    /// # Returns
    /// This facade with the selected timeout.
    #[inline(always)]
    pub fn hard_attempt_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.hard_attempt_timeout(timeout);
        self
    }

    /// Sets the wall-clock timeout for the entire flow.
    ///
    /// # Parameters
    /// - `timeout`: Hard duration for the complete retry flow.
    ///
    /// # Returns
    /// This facade with the selected timeout.
    #[inline(always)]
    pub fn hard_flow_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.hard_flow_timeout(timeout);
        self
    }

    /// Sets the cooperative cancellation token observed by this execution.
    ///
    /// # Parameters
    /// - `token`: Token shared with the caller.
    ///
    /// # Returns
    /// This facade observing the supplied token.
    #[inline(always)]
    pub fn cancellation_token(mut self, token: RetryCancellationToken) -> Self {
        self.inner = self.inner.cancellation_token(token);
        self
    }

    /// Injects a timer and clock, primarily for deterministic tests.
    ///
    /// # Parameters
    /// - `timer`: Shared timer and monotonic clock.
    ///
    /// # Returns
    /// This facade using the supplied timer instead of its Tokio default.
    #[inline(always)]
    pub fn timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.inner = self.inner.timer(timer);
        self
    }

    /// Injects the random source used by backoff jitter.
    ///
    /// # Parameters
    /// - `random_source`: Shared retry random source.
    ///
    /// # Returns
    /// This facade using the supplied random source.
    #[inline(always)]
    pub fn random_source(
        mut self,
        random_source: Arc<dyn RetryRandomSource>,
    ) -> Self {
        self.inner = self.inner.random_source(random_source);
        self
    }

    /// Executes one future per attempt on the current Tokio runtime.
    ///
    /// # Parameters
    /// - `operation`: Factory creating one future for each admitted attempt.
    ///
    /// # Returns
    /// The successful value or terminal retry error.
    pub async fn run<T, F, Fut>(
        &self,
        operation: F,
    ) -> Result<RetrySuccess<T>, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let default_timer = TokioTimer::current();
        self.inner
            .run_with_default_timer(&default_timer, operation)
            .await
    }
}
