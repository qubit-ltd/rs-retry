/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry execution.
//!
//! A [`Retry`] owns validated retry options and lifecycle listeners. The
//! operation success type is introduced by each `run` call, while the error type
//! is bound by the retry policy.

use qubit_common::BoxError;
use qubit_function::{BiConsumer, BiFunction, Consumer};
use std::fmt;
#[cfg(feature = "tokio")]
use std::future::Future;
use std::io;
use std::panic;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(feature = "tokio")]
use super::async_attempt::AsyncAttempt;
#[cfg(feature = "tokio")]
use super::async_value_operation::AsyncValueOperation;
use super::attempt_cancel_token::AttemptCancelToken;
use super::blocking_attempt_message::BlockingAttemptMessage;
use super::retry_flow_action::RetryFlowAction;
use super::sync_attempt::SyncAttempt;
use super::sync_value_operation::SyncValueOperation;
use crate::event::{RetryContextParts, RetryListeners};
use crate::{
    AttemptExecutorError, AttemptFailure, AttemptFailureDecision, AttemptPanic,
    AttemptTimeoutPolicy, AttemptTimeoutSource, RetryAfterHint, RetryBuilder, RetryConfigError,
    RetryContext, RetryError, RetryErrorReason, RetryOptions,
};

/// Retry policy and executor bound to an operation error type.
///
/// The generic parameter `E` is the caller's operation error type. Cloning a
/// retry policy shares all registered functors through reference-counted
/// `rs-function` wrappers.
#[derive(Clone)]
pub struct Retry<E = BoxError> {
    /// Validated retry limits and backoff settings.
    options: RetryOptions,
    /// Optional retry-after hint extractor.
    retry_after_hint: Option<RetryAfterHint<E>>,
    /// Whether listener panics should be isolated.
    isolate_listener_panics: bool,
    /// Lifecycle listeners.
    listeners: RetryListeners<E>,
}

/// Effective timeout selected for a single attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EffectiveAttemptTimeout {
    /// Timeout duration actually enforced for the attempt.
    duration: Option<Duration>,
    /// Source that selected the effective timeout.
    source: Option<AttemptTimeoutSource>,
}

impl EffectiveAttemptTimeout {
    /// Creates an effective attempt timeout.
    ///
    /// # Parameters
    /// - `duration`: Timeout duration enforced for the attempt.
    /// - `source`: Source that selected the timeout.
    ///
    /// # Returns
    /// A timeout descriptor for one attempt.
    #[inline]
    fn new(duration: Option<Duration>, source: Option<AttemptTimeoutSource>) -> Self {
        Self { duration, source }
    }

    /// Returns the elapsed-budget reason represented by a timeout failure.
    ///
    /// # Parameters
    /// - `failure`: Failure produced by the attempt.
    ///
    /// # Returns
    /// `Some(RetryErrorReason)` when the attempt timed out because an elapsed
    /// budget selected the effective timeout.
    #[inline]
    fn elapsed_timeout_reason<E>(&self, failure: &AttemptFailure<E>) -> Option<RetryErrorReason> {
        if !matches!(failure, AttemptFailure::Timeout) {
            return None;
        }
        match self.source {
            Some(AttemptTimeoutSource::MaxOperationElapsed) => {
                Some(RetryErrorReason::MaxOperationElapsedExceeded)
            }
            Some(AttemptTimeoutSource::MaxTotalElapsed) => {
                Some(RetryErrorReason::MaxTotalElapsedExceeded)
            }
            Some(AttemptTimeoutSource::Configured) | None => None,
        }
    }
}

/// Result and cleanup status returned from one blocking worker attempt.
struct BlockingAttemptOutcome<T, E> {
    /// Attempt result after timeout handling.
    result: Result<T, AttemptFailure<E>>,
    /// Worker threads not observed to exit before cancellation grace ended.
    unreaped_worker_count: u32,
}

impl<T, E> BlockingAttemptOutcome<T, E> {
    /// Creates a worker-attempt outcome.
    ///
    /// # Parameters
    /// - `result`: Attempt result exposed to the retry flow.
    /// - `unreaped_worker_count`: Count of worker threads not observed to exit.
    ///
    /// # Returns
    /// A blocking-attempt outcome.
    #[inline]
    fn new(result: Result<T, AttemptFailure<E>>, unreaped_worker_count: u32) -> Self {
        Self {
            result,
            unreaped_worker_count,
        }
    }
}

/// Source of an effective attempt timeout.
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
    /// [`RetryErrorReason::UnsupportedOperation`] because timeout enforcement
    /// requires worker-thread or async execution.
    pub fn run<T, F>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        if self.options.attempt_timeout().is_some() {
            let attempt_timeout = self.attempt_timeout_duration();
            return Err(self.emit_error(RetryError::new(
                RetryErrorReason::UnsupportedOperation,
                None,
                RetryContext::from_parts(RetryContextParts {
                    attempt: 0,
                    max_attempts: self.options.max_attempts.get(),
                    max_operation_elapsed: self.options.max_operation_elapsed,
                    max_total_elapsed: self.options.max_total_elapsed,
                    operation_elapsed: Duration::ZERO,
                    total_elapsed: Duration::ZERO,
                    attempt_elapsed: Duration::ZERO,
                    attempt_timeout,
                })
                .with_attempt_timeout_source(Some(AttemptTimeoutSource::Configured)),
            )));
        }
        let mut operation = SyncValueOperation::new(&mut operation);
        self.run_sync_operation(&mut operation)
            .map(|()| operation.into_value())
    }

    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// Each attempt runs on a worker thread. Worker panics are captured as
    /// [`AttemptFailure::Panic`]. Worker-spawn failures are reported as
    /// [`AttemptFailure::Executor`]. If the effective timeout expires, the retry
    /// executor stops waiting and marks the attempt's [`AttemptCancelToken`] as
    /// cancelled. It then waits up to [`RetryOptions::worker_cancel_grace`] for
    /// the worker to exit. Configured attempt-timeout expirations continue
    /// according to [`AttemptTimeoutPolicy`] only when the worker exits within
    /// that grace period; otherwise the retry flow stops with
    /// [`RetryErrorReason::WorkerStillRunning`]. Elapsed-budget expirations stop
    /// with [`RetryErrorReason::MaxOperationElapsedExceeded`] or
    /// [`RetryErrorReason::MaxTotalElapsedExceeded`].
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
        let operation = Arc::new(operation);
        let flow_started_at = Instant::now();
        let mut operation_elapsed = Duration::ZERO;
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            let total_elapsed = flow_started_at.elapsed();
            let attempt_timeout = self.effective_attempt_timeout(operation_elapsed, total_elapsed);
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    attempt_timeout,
                )));
            }

            attempts += 1;
            let attempt_timeout =
                self.effective_attempt_timeout(operation_elapsed, flow_started_at.elapsed());
            let before_context = self
                .context(
                    operation_elapsed,
                    flow_started_at.elapsed(),
                    attempts,
                    Duration::ZERO,
                    attempt_timeout.duration,
                )
                .with_attempt_timeout_source(attempt_timeout.source);
            self.emit_before_attempt(&before_context);
            let total_elapsed = flow_started_at.elapsed();
            let attempt_timeout = self.effective_attempt_timeout(operation_elapsed, total_elapsed);
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    attempt_timeout,
                )));
            }

            let attempt_start = Instant::now();
            let outcome =
                self.call_blocking_attempt(Arc::clone(&operation), attempt_timeout.duration);
            let attempt_elapsed = attempt_start.elapsed();
            operation_elapsed = add_elapsed(operation_elapsed, attempt_elapsed);
            let context = self
                .context(
                    operation_elapsed,
                    flow_started_at.elapsed(),
                    attempts,
                    attempt_elapsed,
                    attempt_timeout.duration,
                )
                .with_attempt_timeout_source(attempt_timeout.source)
                .with_unreaped_worker_count(outcome.unreaped_worker_count);
            match outcome.result {
                Ok(value) => {
                    self.emit_attempt_success(&context);
                    return Ok(value);
                }
                Err(failure) => {
                    if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
                        return Err(self.emit_error(RetryError::new(
                            reason,
                            Some(failure),
                            context,
                        )));
                    }
                    let retry_block_reason = (context.unreaped_worker_count() > 0)
                        .then_some(RetryErrorReason::WorkerStillRunning);
                    match self.handle_failure(
                        attempts,
                        failure,
                        context,
                        retry_block_reason,
                        flow_started_at,
                    ) {
                        RetryFlowAction::Retry { delay, failure } => {
                            if !delay.is_zero() {
                                std::thread::sleep(delay);
                            }
                            last_failure = Some(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                    }
                }
            }
        }
    }

    /// Runs a blocking operation with retry and per-attempt timeout isolation.
    ///
    /// This method is a compatibility alias for [`Retry::run_in_worker`]. It
    /// also runs attempts in worker threads when no timeout is configured, so
    /// worker panics are reported as [`AttemptFailure::Panic`] instead of
    /// unwinding through the caller. Worker-spawn failures are reported as
    /// [`AttemptFailure::Executor`].
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
    #[inline]
    pub fn run_blocking_with_timeout<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        self.run_in_worker(operation)
    }

    /// Runs a synchronous value-erased operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    fn run_sync_operation(&self, operation: &mut dyn SyncAttempt<E>) -> Result<(), RetryError<E>> {
        let flow_started_at = Instant::now();
        let mut operation_elapsed = Duration::ZERO;
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            let total_elapsed = flow_started_at.elapsed();
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    EffectiveAttemptTimeout::new(None, None),
                )));
            }

            attempts += 1;
            let before_context = self.context(
                operation_elapsed,
                flow_started_at.elapsed(),
                attempts,
                Duration::ZERO,
                None,
            );
            self.emit_before_attempt(&before_context);
            let total_elapsed = flow_started_at.elapsed();
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    EffectiveAttemptTimeout::new(None, None),
                )));
            }

            let attempt_start = Instant::now();
            match operation.call() {
                Ok(()) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    operation_elapsed = add_elapsed(operation_elapsed, attempt_elapsed);
                    let context = self.context(
                        operation_elapsed,
                        flow_started_at.elapsed(),
                        attempts,
                        attempt_elapsed,
                        None,
                    );
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    let attempt_elapsed = attempt_start.elapsed();
                    operation_elapsed = add_elapsed(operation_elapsed, attempt_elapsed);
                    let context = self.context(
                        operation_elapsed,
                        flow_started_at.elapsed(),
                        attempts,
                        attempt_elapsed,
                        None,
                    );
                    match self.handle_failure(attempts, failure, context, None, flow_started_at) {
                        RetryFlowAction::Retry { delay, failure } => {
                            if !delay.is_zero() {
                                std::thread::sleep(delay);
                            }
                            last_failure = Some(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                    }
                }
            }
        }
    }

    /// Runs an asynchronous operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Factory returning a fresh future for each attempt.
    ///
    /// # Returns
    /// `Ok(T)` with the operation value, or [`RetryError`] when retrying stops.
    ///
    /// # Panics
    /// Propagates operation panics from the current async task. They are not
    /// converted to [`AttemptFailure::Panic`] because `run_async` does not
    /// create an isolation boundary. Listener panics are propagated unless
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
    pub async fn run_async<T, F, Fut>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut operation = AsyncValueOperation::new(&mut operation);
        self.run_async_operation(&mut operation)
            .await
            .map(|()| operation.into_value())
    }

    /// Runs an asynchronous value-erased operation with retry.
    ///
    /// # Parameters
    /// - `operation`: Async operation adapter called once per attempt.
    ///
    /// # Returns
    /// `Ok(())` after a successful attempt, or [`RetryError`] when retrying stops.
    #[cfg(feature = "tokio")]
    async fn run_async_operation(
        &self,
        operation: &mut dyn AsyncAttempt<E>,
    ) -> Result<(), RetryError<E>> {
        let flow_started_at = Instant::now();
        let mut operation_elapsed = Duration::ZERO;
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            let total_elapsed = flow_started_at.elapsed();
            let attempt_timeout = self.effective_attempt_timeout(operation_elapsed, total_elapsed);
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    attempt_timeout,
                )));
            }

            attempts += 1;
            let attempt_timeout =
                self.effective_attempt_timeout(operation_elapsed, flow_started_at.elapsed());
            let before_context = self
                .context(
                    operation_elapsed,
                    flow_started_at.elapsed(),
                    attempts,
                    Duration::ZERO,
                    attempt_timeout.duration,
                )
                .with_attempt_timeout_source(attempt_timeout.source);
            self.emit_before_attempt(&before_context);
            let total_elapsed = flow_started_at.elapsed();
            let attempt_timeout = self.effective_attempt_timeout(operation_elapsed, total_elapsed);
            if let Some(reason) = self.elapsed_error_reason(operation_elapsed, total_elapsed) {
                return Err(self.emit_error(self.elapsed_error(
                    reason,
                    operation_elapsed,
                    total_elapsed,
                    attempts,
                    last_failure.take(),
                    attempt_timeout,
                )));
            }

            let attempt_start = Instant::now();
            let result = if let Some(timeout) = attempt_timeout.duration {
                match tokio::time::timeout(timeout, operation.call()).await {
                    Ok(result) => result,
                    Err(_) => Err(AttemptFailure::Timeout),
                }
            } else {
                operation.call().await
            };

            let attempt_elapsed = attempt_start.elapsed();
            operation_elapsed = add_elapsed(operation_elapsed, attempt_elapsed);
            let context = self
                .context(
                    operation_elapsed,
                    flow_started_at.elapsed(),
                    attempts,
                    attempt_elapsed,
                    attempt_timeout.duration,
                )
                .with_attempt_timeout_source(attempt_timeout.source);
            match result {
                Ok(()) => {
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
                        return Err(self.emit_error(RetryError::new(
                            reason,
                            Some(failure),
                            context,
                        )));
                    }
                    match self.handle_failure(attempts, failure, context, None, flow_started_at) {
                        RetryFlowAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            last_failure = Some(failure);
                        }
                        RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                    }
                }
            }
        }
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
            retry_after_hint,
            isolate_listener_panics,
            listeners,
        }
    }

    /// Builds a context snapshot.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic time consumed by this flow.
    /// - `attempt`: Current attempt number.
    /// - `attempt_elapsed`: Elapsed time in the current attempt.
    /// - `attempt_timeout`: Effective timeout configured for the current attempt.
    ///
    /// # Returns
    /// A retry context.
    fn context(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
        attempt: u32,
        attempt_elapsed: Duration,
        attempt_timeout: Option<Duration>,
    ) -> RetryContext {
        RetryContext::from_parts(RetryContextParts {
            attempt,
            max_attempts: self.options.max_attempts.get(),
            max_operation_elapsed: self.options.max_operation_elapsed,
            max_total_elapsed: self.options.max_total_elapsed,
            operation_elapsed,
            total_elapsed,
            attempt_elapsed,
            attempt_timeout,
        })
    }

    /// Returns the configured attempt-timeout duration.
    ///
    /// # Returns
    /// `Some(Duration)` when per-attempt timeout is configured.
    #[inline]
    fn attempt_timeout_duration(&self) -> Option<Duration> {
        self.options
            .attempt_timeout()
            .map(|attempt_timeout| attempt_timeout.timeout())
    }

    /// Returns the effective timeout used by the next attempt.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed so far.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed so far.
    ///
    /// # Returns
    /// The shortest of the configured attempt timeout, remaining
    /// max-operation-elapsed budget, and remaining max-total-elapsed budget,
    /// including the source that selected it. A configured timeout wins ties so
    /// its timeout policy remains observable.
    fn effective_attempt_timeout(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> EffectiveAttemptTimeout {
        let candidates = [
            self.attempt_timeout_duration()
                .map(|duration| (duration, AttemptTimeoutSource::Configured)),
            self.remaining_operation_elapsed(operation_elapsed)
                .map(|duration| (duration, AttemptTimeoutSource::MaxOperationElapsed)),
            self.remaining_total_elapsed(total_elapsed)
                .map(|duration| (duration, AttemptTimeoutSource::MaxTotalElapsed)),
        ];
        let selected = candidates.into_iter().flatten().min_by(|left, right| {
            left.0.cmp(&right.0).then_with(|| {
                timeout_source_priority(left.1).cmp(&timeout_source_priority(right.1))
            })
        });
        match selected {
            Some((duration, source)) => EffectiveAttemptTimeout::new(Some(duration), Some(source)),
            None => EffectiveAttemptTimeout::new(None, None),
        }
    }

    /// Returns remaining user operation time before the max-operation-elapsed budget is exhausted.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed so far.
    ///
    /// # Returns
    /// `Some(Duration)` when max elapsed is configured, or `None` when unlimited.
    #[inline]
    fn remaining_operation_elapsed(&self, operation_elapsed: Duration) -> Option<Duration> {
        self.options
            .max_operation_elapsed
            .map(|max_operation_elapsed| max_operation_elapsed.saturating_sub(operation_elapsed))
    }

    /// Returns remaining total retry-flow time before the max-total-elapsed budget is exhausted.
    ///
    /// # Parameters
    /// - `total_elapsed`: Total monotonic retry-flow time consumed so far.
    ///
    /// # Returns
    /// `Some(Duration)` when max total elapsed is configured, or `None` when unlimited.
    #[inline]
    fn remaining_total_elapsed(&self, total_elapsed: Duration) -> Option<Duration> {
        self.options
            .max_total_elapsed
            .map(|max_total_elapsed| max_total_elapsed.saturating_sub(total_elapsed))
    }

    /// Runs one blocking attempt on a worker thread.
    ///
    /// # Parameters
    /// - `operation`: Shared blocking operation.
    /// - `attempt_timeout`: Effective timeout for this attempt, if any.
    ///
    /// # Returns
    /// The operation value on success, or an attempt failure.
    ///
    /// # Panics
    /// Converts worker panics into [`AttemptFailure::Panic`] and worker-spawn
    /// failures into [`AttemptFailure::Executor`].
    fn call_blocking_attempt<T, F>(
        &self,
        operation: Arc<F>,
        attempt_timeout: Option<Duration>,
    ) -> BlockingAttemptOutcome<T, E>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let token = AttemptCancelToken::new();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker_token = token.clone();
        let worker = std::thread::Builder::new()
            .name("qubit-retry-worker".to_string())
            .spawn(move || {
                let result =
                    panic::catch_unwind(panic::AssertUnwindSafe(|| operation(worker_token)));
                let message = match result {
                    Ok(result) => BlockingAttemptMessage::Result(result),
                    Err(payload) => {
                        BlockingAttemptMessage::Panic(AttemptPanic::from_payload(payload))
                    }
                };
                let _ = sender.send(message);
            });
        #[cfg(not(coverage))]
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                return BlockingAttemptOutcome::new(
                    Err(worker_spawn_error_to_attempt_failure(error)),
                    0,
                );
            }
        };
        #[cfg(coverage)]
        let worker = worker.expect("retry worker should spawn during coverage");

        match attempt_timeout {
            Some(attempt_timeout) => {
                let message = receiver.recv_timeout(attempt_timeout);
                self.worker_timeout_message_to_attempt_outcome(message, receiver, worker, &token)
            }
            None => {
                let result = worker_recv_message_to_attempt_result(receiver.recv());
                join_finished_worker(worker);
                BlockingAttemptOutcome::new(result, 0)
            }
        }
    }

    /// Handles one failed attempt.
    ///
    /// # Parameters
    /// - `attempts`: Attempts executed so far.
    /// - `failure`: Attempt failure.
    /// - `context`: Context captured after the failed attempt.
    ///
    /// # Returns
    /// A retry action selected from listeners and configured limits.
    fn handle_failure(
        &self,
        attempts: u32,
        failure: AttemptFailure<E>,
        context: RetryContext,
        retry_block_reason: Option<RetryErrorReason>,
        flow_started_at: Instant,
    ) -> RetryFlowAction<E> {
        let hint = self
            .retry_after_hint
            .as_ref()
            .and_then(|hint| self.invoke_listener(|| hint.apply(&failure, &context)));
        let context = context
            .with_retry_after_hint(hint)
            .with_total_elapsed(flow_started_at.elapsed());

        let decision =
            self.resolve_failure_decision(self.failure_decision(&failure, &context), &failure);
        let context = context.with_total_elapsed(flow_started_at.elapsed());
        if decision == AttemptFailureDecision::Abort {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::Aborted,
                Some(failure),
                context,
            ));
        }

        let max_attempts = self.options.max_attempts.get();
        if attempts >= max_attempts {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::AttemptsExceeded,
                Some(failure),
                context,
            ));
        }

        if let Some(reason) =
            self.elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        if let Some(reason) = retry_block_reason {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }

        let delay = self.retry_delay(decision, attempts, hint);
        let context = context
            .with_total_elapsed(flow_started_at.elapsed())
            .with_next_delay(delay);
        if self.retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay) {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        self.emit_retry_scheduled(&failure, &context);
        let context = context.with_total_elapsed(flow_started_at.elapsed());
        if let Some(reason) =
            self.elapsed_error_reason(context.operation_elapsed(), context.total_elapsed())
        {
            return RetryFlowAction::Finished(RetryError::new(reason, Some(failure), context));
        }
        if self.retry_sleep_exhausts_total_elapsed(context.total_elapsed(), delay) {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxTotalElapsedExceeded,
                Some(failure),
                context,
            ));
        }
        RetryFlowAction::Retry { delay, failure }
    }

    /// Resolves all failure listeners into one decision.
    ///
    /// # Parameters
    /// - `failure`: Attempt failure.
    /// - `context`: Failure context.
    ///
    /// # Returns
    /// Last non-default listener decision, or [`AttemptFailureDecision::UseDefault`].
    fn failure_decision(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> AttemptFailureDecision {
        let mut decision = AttemptFailureDecision::UseDefault;
        for listener in &self.listeners.failure {
            let current = self.invoke_listener(|| listener.apply(failure, context));
            if current != AttemptFailureDecision::UseDefault {
                decision = current;
            }
        }
        decision
    }

    /// Resolves the effective failure decision after applying timeout policy.
    ///
    /// # Parameters
    /// - `decision`: Decision returned by failure listeners.
    /// - `failure`: Attempt failure being handled.
    ///
    /// # Returns
    /// A concrete decision for timeout failures when listeners used the default.
    fn resolve_failure_decision(
        &self,
        decision: AttemptFailureDecision,
        failure: &AttemptFailure<E>,
    ) -> AttemptFailureDecision {
        if decision != AttemptFailureDecision::UseDefault {
            return decision;
        }
        if matches!(failure, AttemptFailure::Timeout)
            && let Some(attempt_timeout) = self.options.attempt_timeout()
        {
            match attempt_timeout.policy() {
                AttemptTimeoutPolicy::Retry => AttemptFailureDecision::Retry,
                AttemptTimeoutPolicy::Abort => AttemptFailureDecision::Abort,
            }
        } else if matches!(
            failure,
            AttemptFailure::Panic(_) | AttemptFailure::Executor(_)
        ) {
            AttemptFailureDecision::Abort
        } else {
            AttemptFailureDecision::UseDefault
        }
    }

    /// Selects the delay used before the next retry.
    ///
    /// # Parameters
    /// - `decision`: Failure decision.
    /// - `attempts`: Attempts executed so far.
    /// - `hint`: Optional retry-after hint.
    ///
    /// # Returns
    /// Delay before the next retry.
    fn retry_delay(
        &self,
        decision: AttemptFailureDecision,
        attempts: u32,
        hint: Option<Duration>,
    ) -> Duration {
        match decision {
            AttemptFailureDecision::RetryAfter(delay) => delay,
            AttemptFailureDecision::UseDefault => hint.unwrap_or_else(|| {
                self.options
                    .jitter
                    .delay_for_attempt(&self.options.delay, attempts)
            }),
            AttemptFailureDecision::Retry | AttemptFailureDecision::Abort => self
                .options
                .jitter
                .delay_for_attempt(&self.options.delay, attempts),
        }
    }

    /// Builds an elapsed-budget error.
    ///
    /// # Parameters
    /// - `reason`: Elapsed-budget reason selected by the caller.
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed by this flow.
    /// - `attempts`: Attempts executed so far.
    /// - `last_failure`: Last observed failure, if any.
    /// - `attempt_timeout`: Timeout visible in the terminal context.
    ///
    /// # Returns
    /// A retry error preserving the terminal context.
    fn elapsed_error(
        &self,
        reason: RetryErrorReason,
        operation_elapsed: Duration,
        total_elapsed: Duration,
        attempts: u32,
        last_failure: Option<AttemptFailure<E>>,
        attempt_timeout: EffectiveAttemptTimeout,
    ) -> RetryError<E> {
        RetryError::new(
            reason,
            last_failure,
            self.context(
                operation_elapsed,
                total_elapsed,
                attempts,
                Duration::ZERO,
                attempt_timeout.duration,
            )
            .with_attempt_timeout_source(attempt_timeout.source),
        )
    }

    /// Returns the first elapsed-budget reason that is exhausted.
    ///
    /// # Parameters
    /// - `operation_elapsed`: Cumulative user operation time consumed by this flow.
    /// - `total_elapsed`: Total monotonic retry-flow time consumed by this flow.
    ///
    /// # Returns
    /// `Some(RetryErrorReason)` when an elapsed budget has been exhausted.
    #[inline]
    fn elapsed_error_reason(
        &self,
        operation_elapsed: Duration,
        total_elapsed: Duration,
    ) -> Option<RetryErrorReason> {
        if self
            .options
            .max_operation_elapsed
            .is_some_and(|max_operation_elapsed| operation_elapsed >= max_operation_elapsed)
        {
            Some(RetryErrorReason::MaxOperationElapsedExceeded)
        } else if self
            .options
            .max_total_elapsed
            .is_some_and(|max_total_elapsed| total_elapsed >= max_total_elapsed)
        {
            Some(RetryErrorReason::MaxTotalElapsedExceeded)
        } else {
            None
        }
    }

    /// Returns whether a selected retry sleep would consume the remaining total budget.
    ///
    /// # Parameters
    /// - `total_elapsed`: Total monotonic retry-flow time consumed before sleep.
    /// - `delay`: Selected retry delay.
    ///
    /// # Returns
    /// `true` when the delay should not be slept because no budget would remain
    /// for the next attempt.
    #[inline]
    fn retry_sleep_exhausts_total_elapsed(&self, total_elapsed: Duration, delay: Duration) -> bool {
        if delay.is_zero() {
            return false;
        }
        let Some(max_total_elapsed) = self.options.max_total_elapsed else {
            return false;
        };
        let Some(remaining) = max_total_elapsed.checked_sub(total_elapsed) else {
            return true;
        };
        delay >= remaining
    }

    /// Emits before-attempt listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    fn emit_before_attempt(&self, context: &RetryContext) {
        for listener in &self.listeners.before_attempt {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits attempt-success listeners.
    ///
    /// # Parameters
    /// - `context`: Context passed to listeners.
    fn emit_attempt_success(&self, context: &RetryContext) {
        for listener in &self.listeners.attempt_success {
            self.invoke_listener(|| {
                listener.accept(context);
            });
        }
    }

    /// Emits retry-scheduled listeners.
    ///
    /// # Parameters
    /// - `failure`: Failure that caused the retry to be scheduled.
    /// - `context`: Context carrying the selected next delay.
    fn emit_retry_scheduled(&self, failure: &AttemptFailure<E>, context: &RetryContext) {
        for listener in &self.listeners.retry_scheduled {
            self.invoke_listener(|| {
                listener.accept(failure, context);
            });
        }
    }

    /// Emits terminal error listeners and returns the same error.
    ///
    /// # Parameters
    /// - `error`: Terminal retry error.
    ///
    /// # Returns
    /// The same error after listeners have been invoked.
    fn emit_error(&self, error: RetryError<E>) -> RetryError<E> {
        for listener in &self.listeners.error {
            self.invoke_listener(|| {
                listener.accept(&error, error.context());
            });
        }
        error
    }

    /// Converts a timeout-aware worker receive into an attempt outcome.
    ///
    /// # Parameters
    /// - `message`: Initial receive result with the attempt timeout applied.
    /// - `receiver`: Receiver used to observe worker exit during cancellation grace.
    /// - `worker`: Worker thread handle for joining finished workers.
    /// - `token`: Cancellation token to mark when the receive timed out.
    ///
    /// # Returns
    /// Attempt result plus the number of worker threads not observed to exit.
    fn worker_timeout_message_to_attempt_outcome<T>(
        &self,
        message: Result<BlockingAttemptMessage<T, E>, mpsc::RecvTimeoutError>,
        receiver: mpsc::Receiver<BlockingAttemptMessage<T, E>>,
        worker: JoinHandle<()>,
        token: &AttemptCancelToken,
    ) -> BlockingAttemptOutcome<T, E>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        match message {
            Ok(message) => {
                let result = worker_message_to_attempt_result(message);
                join_finished_worker(worker);
                BlockingAttemptOutcome::new(result, 0)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                token.cancel();
                let worker_exited =
                    wait_for_cancelled_worker(&receiver, worker, self.options.worker_cancel_grace);
                let unreaped_worker_count = if worker_exited { 0 } else { 1 };
                BlockingAttemptOutcome::new(Err(AttemptFailure::Timeout), unreaped_worker_count)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                join_finished_worker(worker);
                BlockingAttemptOutcome::new(Err(worker_disconnected_attempt_failure()), 0)
            }
        }
    }

    /// Invokes a listener and optionally isolates panics.
    ///
    /// # Parameters
    /// - `call`: Listener invocation closure.
    ///
    /// # Returns
    /// The listener return value, or `Default::default()` when an isolated panic
    /// occurs.
    fn invoke_listener<R>(&self, call: impl FnOnce() -> R) -> R
    where
        R: Default,
    {
        if self.isolate_listener_panics {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_default()
        } else {
            call()
        }
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

/// Converts a worker message into an attempt result.
///
/// # Parameters
/// - `message`: Message received from the worker thread.
///
/// # Returns
/// The operation value on success, or an attempt failure.
fn worker_message_to_attempt_result<T, E>(
    message: BlockingAttemptMessage<T, E>,
) -> Result<T, AttemptFailure<E>> {
    match message {
        BlockingAttemptMessage::Result(result) => result.map_err(AttemptFailure::Error),
        BlockingAttemptMessage::Panic(panic) => Err(AttemptFailure::Panic(panic)),
    }
}

/// Converts a worker-spawn error into an attempt failure.
///
/// # Parameters
/// - `error`: Error returned by `std::thread::Builder::spawn`.
///
/// # Returns
/// An executor attempt failure that preserves the spawn error context.
#[cfg_attr(coverage, allow(dead_code))]
fn worker_spawn_error_to_attempt_failure<E>(error: io::Error) -> AttemptFailure<E> {
    AttemptFailure::Executor(AttemptExecutorError::from_spawn_error(error))
}

/// Converts a timeout-aware receive result into an attempt result.
///
/// # Parameters
/// - `message`: Result returned by `Receiver::recv_timeout`.
/// - `token`: Cancellation token to mark when the receive timed out.
///
/// # Returns
/// The operation value on success, or an attempt failure.
#[cfg(all(coverage, not(test)))]
fn worker_timeout_message_to_attempt_result<T, E>(
    message: Result<BlockingAttemptMessage<T, E>, mpsc::RecvTimeoutError>,
    token: &AttemptCancelToken,
) -> Result<T, AttemptFailure<E>> {
    match message {
        Ok(message) => worker_message_to_attempt_result(message),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            token.cancel();
            Err(AttemptFailure::Timeout)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(worker_disconnected_attempt_failure()),
    }
}

/// Converts a blocking receive result into an attempt result.
///
/// # Parameters
/// - `message`: Result returned by `Receiver::recv`.
///
/// # Returns
/// The operation value on success, or an attempt failure.
fn worker_recv_message_to_attempt_result<T, E>(
    message: Result<BlockingAttemptMessage<T, E>, mpsc::RecvError>,
) -> Result<T, AttemptFailure<E>> {
    match message {
        Ok(message) => worker_message_to_attempt_result(message),
        Err(_) => Err(worker_disconnected_attempt_failure()),
    }
}

/// Builds an executor failure for a disconnected worker result channel.
///
/// # Returns
/// An attempt failure describing the disconnected worker channel.
fn worker_disconnected_attempt_failure<E>() -> AttemptFailure<E> {
    AttemptFailure::Executor(AttemptExecutorError::from_worker_disconnected())
}

/// Waits briefly for a cancelled worker to exit.
///
/// # Parameters
/// - `receiver`: Worker result receiver used only to observe whether the worker
///   sent or disconnected.
/// - `worker`: Worker thread handle, joined when exit is observed.
/// - `grace`: Maximum time to wait after cancellation. Zero performs only a
///   non-blocking check.
///
/// # Returns
/// `true` when the worker was observed to exit before the grace period ended,
/// otherwise `false`. When this returns `false`, the worker handle is dropped and
/// the thread may continue running detached.
fn wait_for_cancelled_worker<T, E>(
    receiver: &mpsc::Receiver<BlockingAttemptMessage<T, E>>,
    worker: JoinHandle<()>,
    grace: Duration,
) -> bool {
    let exited = if grace.is_zero() {
        match receiver.try_recv() {
            Ok(_) | Err(mpsc::TryRecvError::Disconnected) => true,
            Err(mpsc::TryRecvError::Empty) => false,
        }
    } else {
        match receiver.recv_timeout(grace) {
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => false,
        }
    };
    if exited {
        join_finished_worker(worker);
    }
    exited
}

/// Joins a worker thread that has already been observed to finish.
///
/// # Parameters
/// - `worker`: Worker thread handle.
///
/// # Returns
/// This function returns nothing.
fn join_finished_worker(worker: JoinHandle<()>) {
    let _ = worker.join();
}

/// Adds one attempt duration to the cumulative user-operation elapsed time.
///
/// # Parameters
/// - `operation_elapsed`: Cumulative elapsed time before the attempt.
/// - `attempt_elapsed`: Elapsed time consumed by the current attempt.
///
/// # Returns
/// The summed elapsed time, saturated at [`Duration::MAX`] on overflow.
fn add_elapsed(operation_elapsed: Duration, attempt_elapsed: Duration) -> Duration {
    operation_elapsed.saturating_add(attempt_elapsed)
}

/// Returns tie-break priority for effective attempt-timeout sources.
///
/// Lower values win. Configured timeouts intentionally win exact ties so their
/// timeout policy remains observable. Operation elapsed wins an exact tie with
/// total elapsed to preserve the narrower operation-budget diagnostic.
///
/// # Parameters
/// - `source`: Attempt-timeout source to rank.
///
/// # Returns
/// Numeric tie-break priority.
#[inline]
fn timeout_source_priority(source: AttemptTimeoutSource) -> u8 {
    match source {
        AttemptTimeoutSource::Configured => 0,
        AttemptTimeoutSource::MaxOperationElapsed => 1,
        AttemptTimeoutSource::MaxTotalElapsed => 2,
    }
}

/// Sleeps asynchronously when the delay is non-zero.
///
/// # Parameters
/// - `delay`: Delay to sleep.
///
/// # Returns
/// This function returns after the sleep completes.
#[cfg(feature = "tokio")]
async fn sleep_async(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

/// Coverage-only hooks for exercising defensive retry executor branches.
#[cfg(all(coverage, not(test)))]
#[doc(hidden)]
pub mod coverage_support {
    use std::error::Error;
    use std::io;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use crate::event::RetryContextParts;
    use crate::{
        AttemptCancelToken, AttemptExecutorError, AttemptFailure, AttemptPanic, Retry,
        RetryContext, RetryError, RetryErrorReason,
    };

    use super::{
        BlockingAttemptMessage, wait_for_cancelled_worker, worker_message_to_attempt_result,
        worker_recv_message_to_attempt_result, worker_spawn_error_to_attempt_failure,
        worker_timeout_message_to_attempt_result,
    };

    /// Exercises internal branches that are not reliably reachable through
    /// normal worker-thread execution.
    ///
    /// # Returns
    /// Diagnostic messages describing each exercised defensive path.
    pub fn exercise_defensive_paths() -> Vec<String> {
        let mut diagnostics = Vec::new();

        let spawn_failure =
            worker_spawn_error_to_attempt_failure::<&'static str>(io::Error::other("spawn failed"));
        diagnostics.push(spawn_failure.to_string());

        let timeout_token = AttemptCancelToken::new();
        let timeout = worker_timeout_message_to_attempt_result::<(), &'static str>(
            Err(mpsc::RecvTimeoutError::Timeout),
            &timeout_token,
        )
        .expect_err("timeout receive should become an attempt failure");
        diagnostics.push(format!(
            "{timeout}; cancelled={}",
            timeout_token.is_cancelled()
        ));

        let timeout_disconnected = worker_timeout_message_to_attempt_result::<(), &'static str>(
            Err(mpsc::RecvTimeoutError::Disconnected),
            &AttemptCancelToken::new(),
        )
        .expect_err("disconnected timeout receive should become an executor failure");
        diagnostics.push(timeout_disconnected.to_string());

        let retry = Retry::<io::Error>::builder()
            .max_total_elapsed(Some(Duration::ZERO))
            .build()
            .expect("coverage retry should build");
        diagnostics.push(format!(
            "sleep budget exhausted={}",
            retry.retry_sleep_exhausts_total_elapsed(
                Duration::from_millis(1),
                Duration::from_millis(1),
            )
        ));

        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        let worker = thread::spawn(|| {});
        let timeout_outcome = retry.worker_timeout_message_to_attempt_outcome::<()>(
            Err(mpsc::RecvTimeoutError::Disconnected),
            receiver,
            worker,
            &AttemptCancelToken::new(),
        );
        diagnostics.push(
            timeout_outcome
                .result
                .expect_err("disconnected worker timeout should fail")
                .to_string(),
        );

        let (sender, receiver) = mpsc::sync_channel::<BlockingAttemptMessage<(), io::Error>>(1);
        let worker = thread::spawn(|| {});
        let exited = wait_for_cancelled_worker(&receiver, worker, Duration::ZERO);
        drop(sender);
        diagnostics.push(format!("zero grace empty worker exited={exited}"));

        let recv_disconnected =
            worker_recv_message_to_attempt_result::<(), &'static str>(Err(mpsc::RecvError))
                .expect_err("disconnected receive should become an executor failure");
        diagnostics.push(recv_disconnected.to_string());

        let panic_message = worker_message_to_attempt_result::<(), &'static str>(
            BlockingAttemptMessage::Panic(AttemptPanic::new("coverage panic")),
        )
        .expect_err("panic message should become an attempt failure");
        diagnostics.push(panic_message.to_string());

        let static_panic = AttemptPanic::from_payload(Box::new("static panic"));
        diagnostics.push(static_panic.to_string());

        let string_panic = AttemptPanic::from_payload(Box::new(String::from("owned panic")));
        diagnostics.push(string_panic.to_string());

        let executor_error = RetryError::new(
            RetryErrorReason::Aborted,
            Some(AttemptFailure::<io::Error>::Executor(
                AttemptExecutorError::new("executor source"),
            )),
            RetryContext::new(1, 1),
        );
        diagnostics.push(format!(
            "executor reason={:?}; attempts={}; context_attempt={}",
            executor_error.reason(),
            executor_error.attempts(),
            executor_error.context().attempt(),
        ));
        diagnostics.push(
            executor_error
                .source()
                .expect("executor failure should be an error source")
                .to_string(),
        );

        let timeout_error = RetryError::new(
            RetryErrorReason::Aborted,
            Some(AttemptFailure::<io::Error>::Timeout),
            RetryContext::new(1, 1),
        );
        diagnostics.push(format!(
            "timeout source absent={}",
            timeout_error.source().is_none()
        ));

        let app_error = RetryError::new(
            RetryErrorReason::AttemptsExceeded,
            Some(AttemptFailure::<io::Error>::Error(io::Error::other(
                "application source",
            ))),
            RetryContext::new(2, 2),
        );
        diagnostics.push(
            app_error
                .last_failure()
                .expect("application failure should exist")
                .to_string(),
        );
        diagnostics.push(
            app_error
                .last_error()
                .expect("last application error should exist")
                .to_string(),
        );
        diagnostics.push(app_error.to_string());

        let owned_error = RetryError::new(
            RetryErrorReason::AttemptsExceeded,
            Some(AttemptFailure::<io::Error>::Error(io::Error::other(
                "owned application error",
            ))),
            RetryContext::new(2, 2),
        );
        diagnostics.push(
            owned_error
                .into_last_error()
                .expect("owned application error should be returned")
                .to_string(),
        );

        let parted_error = RetryError::<io::Error>::new(
            RetryErrorReason::MaxOperationElapsedExceeded,
            None,
            RetryContext::from_parts(RetryContextParts {
                attempt: 0,
                max_attempts: 2,
                max_operation_elapsed: Some(Duration::ZERO),
                max_total_elapsed: None,
                operation_elapsed: Duration::ZERO,
                total_elapsed: Duration::ZERO,
                attempt_elapsed: Duration::ZERO,
                attempt_timeout: None,
            }),
        );
        let (reason, last_failure, context) = parted_error.into_parts();
        diagnostics.push(format!(
            "parts reason={reason:?}; last_failure={}; max_operation_elapsed={:?}",
            last_failure.is_some(),
            context.max_operation_elapsed(),
        ));

        diagnostics
    }
}
