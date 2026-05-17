/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry policy facade and public execution entry point.
//!
//! A [`Retry`] owns validated retry options and lifecycle events. Execution
//! mode details live in dedicated runner objects. This keeps the public API
//! centered on "configure a policy, then run an operation" while sync, async,
//! and worker-thread mechanics remain private executor responsibilities.

use std::fmt;
#[cfg(feature = "tokio")]
use std::future::Future;

use qubit_error::BoxError;

#[cfg(feature = "tokio")]
use super::async_retry_runner::AsyncRetryRunner;
use super::attempt_cancel_token::AttemptCancelToken;
use super::retry_builder::RetryBuilder;
use super::retry_runner::RetryRunner;
use super::worker_retry_runner::WorkerRetryRunner;
use crate::event::{
    RetryEvents,
    RetryListeners,
};
use crate::{
    RetryAfterHint,
    RetryConfigError,
    RetryError,
    RetryOptions,
};

/// Retry policy and executor facade bound to an operation error type.
///
/// The generic parameter `E` is the caller's operation error type. Cloning a
/// retry policy shares all registered functors through reference-counted
/// `rs-function` wrappers.
///
/// # Architecture
///
/// `Retry` is the public facade of the retry engine. It owns two stable pieces
/// of state:
///
/// - [`RetryOptions`], which describes the policy: attempt limits, elapsed
///   budgets, backoff, retry-after handling, per-attempt timeout, and
///   worker-cancellation grace.
/// - `RetryEvents`, the internal dispatcher for listener callbacks and
///   retry-after hints.
///
/// The facade deliberately contains no retry loop logic. Each public execution
/// method creates a mode-specific runner and delegates the real control flow:
///
/// - [`Retry::run`] uses `RetryRunner` and executes the caller's closure on the
///   current thread.
/// - `Retry::run_async` (requires the `tokio` feature) uses `AsyncRetryRunner`
///   and executes each future on the current Tokio task.
/// - [`Retry::run_in_worker`] uses `WorkerRetryRunner` and executes each
///   attempt inside a worker thread.
///
/// Those runners all follow the same conceptual pipeline. They adapt the
/// caller's operation into an internal attempt object, keep a
/// `RetryFlowState`, fire lifecycle events, call the operation once per
/// attempt, and pass failures to `RetryFailureHandler` to decide whether to
/// sleep and retry or return a terminal [`RetryError`]. The mode-specific runner
/// owns only the execution mechanics that differ by mode: blocking sleep,
/// Tokio timeout, worker-thread panic capture, and cooperative cancellation.
/// This split keeps the public API small while keeping timeout and concurrency
/// details out of the `Retry` facade.
#[derive(Clone)]
pub struct Retry<E = BoxError> {
    /// Validated retry limits and backoff settings.
    options: RetryOptions,
    /// Retry lifecycle event dispatcher.
    events: RetryEvents<E>,
}

#[allow(clippy::result_large_err)]
impl<E> Retry<E> {
    /// Creates a retry builder.
    ///
    /// # Returns
    /// A [`RetryBuilder`] configured with defaults.
    #[inline]
    pub fn builder() -> RetryBuilder<E> {
        RetryBuilder::new()
    }

    /// Creates a retry policy from options.
    ///
    /// # Parameters
    /// - `options`: Retry options to validate and install.
    ///
    /// # Returns
    /// A retry policy using the default listener set.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] if the options are invalid.
    pub fn from_options(options: RetryOptions) -> Result<Self, RetryConfigError> {
        Self::builder().options(options).build()
    }

    /// Returns the immutable options used by this retry policy.
    ///
    /// # Returns
    /// Shared retry options.
    #[inline]
    pub fn options(&self) -> &RetryOptions {
        &self.options
    }

    /// Runs a synchronous operation with retry.
    ///
    /// This method is the same-thread execution path. The call flow is:
    ///
    /// 1. Create a `RetryRunner` that borrows this retry policy.
    /// 2. Wrap `operation` in a value-capturing internal adapter so the retry
    ///    loop can work with a type-erased `Result<(), AttemptFailure<E>>`
    ///    while preserving the successful `T` value.
    /// 3. Reject configured `attempt_timeout`, because a same-thread closure
    ///    cannot be interrupted safely once it starts running.
    /// 4. For each attempt, update `RetryFlowState`, fire `before_attempt`,
    ///    call the closure directly on the current thread, record elapsed
    ///    operation time, and fire success or failure events.
    /// 5. On failure, let `RetryFailureHandler` apply retry limits, error
    ///    predicates, retry-after hints, elapsed budgets, and backoff. If it
    ///    chooses retry, sleep with `std::thread::sleep` and start the next
    ///    attempt; otherwise return the produced [`RetryError`].
    ///
    /// # Parameters
    /// - `operation`: Operation called once per attempt until it succeeds or the
    ///   retry flow stops.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Propagates operation panics and listener panics unless listener panic
    /// isolation is enabled.
    ///
    /// # Blocking
    /// Blocks the current thread with `std::thread::sleep` between attempts when
    /// a non-zero retry delay is selected.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time, including
    /// operation execution, retry sleep, retry-after sleep, and retry
    /// control-path listener time. This synchronous mode cannot interrupt an
    /// already-running operation; it checks budgets before attempts and after
    /// failed attempts. If `attempt_timeout` is configured, this method returns
    /// [`crate::RetryErrorReason::UnsupportedOperation`] because timeout
    /// enforcement requires worker-thread or async execution.
    pub fn run<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        RetryRunner::new(self).run(operation)
    }

    /// Runs an asynchronous operation with retry.
    ///
    /// This method is the Tokio execution path. The call flow is:
    ///
    /// 1. Create an `AsyncRetryRunner` that borrows this retry policy.
    /// 2. Wrap `operation` in an async value-capturing adapter. The operation is
    ///    a factory: it must create a fresh future for every attempt because a
    ///    Rust future cannot be polled again after it completes.
    /// 3. Before each attempt, compute the effective timeout from the configured
    ///    `attempt_timeout`, remaining `max_operation_elapsed`, and remaining
    ///    `max_total_elapsed`; the shortest available budget wins.
    /// 4. Fire `before_attempt`, recompute budgets in case listeners consumed
    ///    total elapsed time, then await the attempt future. If an effective
    ///    timeout exists, the future is wrapped in `tokio::time::timeout` and
    ///    dropped when the timer fires.
    /// 5. Record elapsed operation time, fire success events, or route the
    ///    failure through elapsed-budget classification and
    ///    `RetryFailureHandler`. Retry delays use `tokio::time::sleep`; terminal
    ///    decisions return [`RetryError`].
    ///
    /// # Parameters
    /// - `operation`: Factory returning a fresh future for each attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Propagates operation panics from the current async task. They are not
    /// converted to [`crate::AttemptFailure::Panic`] because `run_async` does
    /// not create an isolation boundary. Listener panics are propagated unless
    /// listener panic isolation is enabled. Tokio may panic if timer APIs are
    /// used outside a runtime with a time driver.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time. Async attempts use
    /// the shortest of configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget as
    /// their effective timeout.
    #[cfg(feature = "tokio")]
    pub async fn run_async<T, F, Fut>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        AsyncRetryRunner::new(self).run(operation).await
    }

    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// This method is the blocking-isolation execution path. The call flow is:
    ///
    /// 1. Create a `WorkerRetryRunner` that borrows this retry policy.
    /// 2. Wrap the operation in a shared blocking adapter. The adapter stores
    ///    the successful `T` value outside the type-erased retry loop.
    /// 3. For each attempt, compute the effective timeout from configured
    ///    `attempt_timeout` and remaining elapsed budgets, fire
    ///    `before_attempt`, then spawn one worker-thread attempt through
    ///    `WorkerAttemptExecutor`.
    /// 4. The worker receives an [`AttemptCancelToken`]. If the effective
    ///    timeout expires, the runner marks that token as cancelled and waits up
    ///    to [`crate::RetryOptions::worker_cancel_grace`] for cooperative exit.
    /// 5. Worker panics become [`crate::AttemptFailure::Panic`], worker-spawn
    ///    failures become [`crate::AttemptFailure::Executor`], and timeout or
    ///    application failures are passed through the normal retry policy.
    /// 6. The runner refuses to start another worker while a timed-out worker is
    ///    still running, because that would create concurrent attempts for one
    ///    retry flow. That condition returns
    ///    [`crate::RetryErrorReason::WorkerStillRunning`].
    ///
    /// Each attempt runs on a worker thread. Worker panics are captured as
    /// [`crate::AttemptFailure::Panic`]. Worker-spawn failures are reported as
    /// [`crate::AttemptFailure::Executor`]. If the effective timeout expires,
    /// the retry executor stops waiting and marks the attempt's
    /// [`AttemptCancelToken`] as cancelled. It then waits up to
    /// [`crate::RetryOptions::worker_cancel_grace`] for the worker to exit.
    /// Configured attempt-timeout expirations continue according to
    /// [`crate::AttemptTimeoutPolicy`] only when the worker exits within that
    /// grace period; otherwise the retry flow stops with
    /// [`crate::RetryErrorReason::WorkerStillRunning`]. Elapsed-budget
    /// expirations stop with
    /// [`crate::RetryErrorReason::MaxOperationElapsedExceeded`] or
    /// [`crate::RetryErrorReason::MaxTotalElapsedExceeded`].
    ///
    /// # Parameters
    /// - `operation`: Thread-safe operation called once per attempt. It receives
    ///   a cooperative cancellation token for that attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Does not propagate operation panics. Listener panic behavior follows this
    /// retry policy's listener isolation setting.
    ///
    /// # Blocking
    /// Blocks the current thread while waiting for each worker result or timeout
    /// and while sleeping between retry attempts.
    ///
    /// # Elapsed Budget
    /// `max_operation_elapsed` counts only user operation execution time.
    /// `max_total_elapsed` counts monotonic retry-flow time. Worker attempts use
    /// the shortest of configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget as
    /// their effective timeout.
    pub fn run_in_worker<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        WorkerRetryRunner::new(self).run(operation)
    }

    /// Creates a retry policy from validated parts.
    ///
    /// # Parameters
    /// - `options`: Retry options.
    /// - `retry_after_hint`: Optional hint extractor.
    /// - `isolate_listener_panics`: Whether listener panics are isolated.
    /// - `listeners`: Lifecycle listeners.
    ///
    /// # Returns
    /// A retry policy.
    pub(super) fn new(
        options: RetryOptions,
        retry_after_hint: Option<RetryAfterHint<E>>,
        isolate_listener_panics: bool,
        listeners: RetryListeners<E>,
    ) -> Self {
        Self {
            options,
            events: RetryEvents::new(retry_after_hint, isolate_listener_panics, listeners),
        }
    }

    /// Returns the internal event dispatcher.
    ///
    /// # Returns
    /// Event dispatcher used by retry runners.
    #[inline]
    pub(in crate::executor) fn events(&self) -> &RetryEvents<E> {
        &self.events
    }
}

impl<E> fmt::Debug for Retry<E> {
    /// Formats the retry policy without exposing callbacks.
    ///
    /// # Parameters
    /// - `f`: Formatter.
    ///
    /// # Returns
    /// Formatter result.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Retry")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}
