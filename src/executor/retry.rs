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

use std::fmt;
#[cfg(feature = "tokio")]
use std::future::Future;
use std::panic;
#[cfg(feature = "tokio")]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use qubit_common::BoxError;
use qubit_function::{BiConsumer, BiFunction, Consumer};

use super::attempt_cancel_token::AttemptCancelToken;
use crate::event::RetryListeners;
use crate::{
    AttemptFailure, AttemptFailureDecision, AttemptPanic, AttemptTimeoutPolicy, RetryAfterHint,
    RetryBuilder, RetryConfigError, RetryContext, RetryError, RetryErrorReason, RetryOptions,
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
    pub fn run<T, F>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let mut operation = SyncValueOperation::new(&mut operation);
        self.run_sync_operation(&mut operation)?;
        Ok(operation.into_value())
    }

    /// Runs a blocking operation with retry inside worker-thread attempts.
    ///
    /// Each attempt runs on a worker thread. Worker panics are captured as
    /// [`AttemptFailure::Panic`]. If the configured attempt timeout expires,
    /// the retry executor stops waiting, marks the attempt's
    /// [`AttemptCancelToken`] as cancelled, and continues according to
    /// [`AttemptTimeoutPolicy`]. The worker thread may continue running if the
    /// operation ignores the cancellation token.
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
    pub fn run_in_worker<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let operation = Arc::new(operation);
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            let attempt_timeout = self.attempt_timeout_duration();
            if let Some(error) =
                self.elapsed_error(start, attempts, last_failure.take(), attempt_timeout)
            {
                return Err(self.emit_error(error));
            }

            attempts += 1;
            let before_context = self.context(start, attempts, Duration::ZERO, attempt_timeout);
            self.emit_before_attempt(&before_context);

            let attempt_start = Instant::now();
            let result = self.call_blocking_attempt(Arc::clone(&operation));
            let context = self.context(start, attempts, attempt_start.elapsed(), attempt_timeout);
            match result {
                Ok(value) => {
                    self.emit_attempt_success(&context);
                    return Ok(value);
                }
                Err(failure) => match self.handle_failure(start, attempts, failure, context) {
                    RetryFlowAction::Retry { delay, failure } => {
                        if !delay.is_zero() {
                            std::thread::sleep(delay);
                        }
                        last_failure = Some(failure);
                    }
                    RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                },
            }
        }
    }

    /// Runs a blocking operation with retry and per-attempt timeout isolation.
    ///
    /// This method is a compatibility alias for [`Retry::run_in_worker`]. It
    /// also runs attempts in worker threads when no timeout is configured, so
    /// worker panics are reported as [`AttemptFailure::Panic`] instead of
    /// unwinding through the caller.
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
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            if let Some(error) = self.elapsed_error(start, attempts, last_failure.take(), None) {
                return Err(self.emit_error(error));
            }

            attempts += 1;
            let before_context = self.context(start, attempts, Duration::ZERO, None);
            self.emit_before_attempt(&before_context);

            let attempt_start = Instant::now();
            match operation.call() {
                Ok(()) => {
                    let context = self.context(start, attempts, attempt_start.elapsed(), None);
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => {
                    let context = self.context(start, attempts, attempt_start.elapsed(), None);
                    match self.handle_failure(start, attempts, failure, context) {
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
    /// Propagates operation panics and listener panics unless listener panic
    /// isolation is enabled. Tokio may panic if timer APIs are used outside a
    /// runtime with a time driver.
    #[cfg(feature = "tokio")]
    pub async fn run_async<T, F, Fut>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut operation = AsyncValueOperation::new(&mut operation);
        self.run_async_operation(&mut operation).await?;
        Ok(operation.into_value())
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
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            let attempt_timeout = self.attempt_timeout_duration();
            if let Some(error) =
                self.elapsed_error(start, attempts, last_failure.take(), attempt_timeout)
            {
                return Err(self.emit_error(error));
            }

            attempts += 1;
            let before_context = self.context(start, attempts, Duration::ZERO, attempt_timeout);
            self.emit_before_attempt(&before_context);

            let attempt_start = Instant::now();
            let result = if let Some(timeout) = attempt_timeout {
                match tokio::time::timeout(timeout, operation.call()).await {
                    Ok(result) => result,
                    Err(_) => Err(AttemptFailure::Timeout),
                }
            } else {
                operation.call().await
            };

            let context = self.context(start, attempts, attempt_start.elapsed(), attempt_timeout);
            match result {
                Ok(()) => {
                    self.emit_attempt_success(&context);
                    return Ok(());
                }
                Err(failure) => match self.handle_failure(start, attempts, failure, context) {
                    RetryFlowAction::Retry { delay, failure } => {
                        sleep_async(delay).await;
                        last_failure = Some(failure);
                    }
                    RetryFlowAction::Finished(error) => return Err(self.emit_error(error)),
                },
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
    /// - `start`: Retry flow start.
    /// - `attempt`: Current attempt number.
    /// - `attempt_elapsed`: Elapsed time in the current attempt.
    /// - `attempt_timeout`: Timeout configured for the current attempt.
    ///
    /// # Returns
    /// A retry context.
    fn context(
        &self,
        start: Instant,
        attempt: u32,
        attempt_elapsed: Duration,
        attempt_timeout: Option<Duration>,
    ) -> RetryContext {
        RetryContext::new(
            attempt,
            self.options.max_attempts.get(),
            self.options.max_elapsed,
            start.elapsed(),
            attempt_elapsed,
            attempt_timeout,
        )
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

    /// Runs one blocking attempt on a worker thread.
    ///
    /// # Parameters
    /// - `operation`: Shared blocking operation.
    ///
    /// # Returns
    /// The operation value on success, or an attempt failure.
    ///
    /// # Panics
    /// Converts worker panics into [`AttemptFailure::Panic`].
    fn call_blocking_attempt<T, F>(&self, operation: Arc<F>) -> Result<T, AttemptFailure<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static,
    {
        let token = AttemptCancelToken::new();
        let worker_token = token.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| operation(worker_token)));
            let message = match result {
                Ok(result) => BlockingAttemptMessage::Result(result),
                Err(payload) => BlockingAttemptMessage::Panic(AttemptPanic::from_payload(payload)),
            };
            let _ = sender.send(message);
        });

        match self.attempt_timeout_duration() {
            Some(attempt_timeout) => match receiver.recv_timeout(attempt_timeout) {
                Ok(message) => worker_message_to_attempt_result(message),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    token.cancel();
                    Err(AttemptFailure::Timeout)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("blocking retry attempt worker stopped without sending a result")
                }
            },
            None => match receiver.recv() {
                Ok(message) => worker_message_to_attempt_result(message),
                Err(mpsc::RecvError) => {
                    panic!("blocking retry attempt worker stopped without sending a result")
                }
            },
        }
    }

    /// Handles one failed attempt.
    ///
    /// # Parameters
    /// - `start`: Retry flow start.
    /// - `attempts`: Attempts executed so far.
    /// - `failure`: Attempt failure.
    /// - `context`: Context captured after the failed attempt.
    ///
    /// # Returns
    /// A retry action selected from listeners and configured limits.
    fn handle_failure(
        &self,
        start: Instant,
        attempts: u32,
        failure: AttemptFailure<E>,
        context: RetryContext,
    ) -> RetryFlowAction<E> {
        let hint = self
            .retry_after_hint
            .as_ref()
            .and_then(|hint| hint.apply(&failure, &context));
        let context = context.with_retry_after_hint(hint);

        let decision =
            self.resolve_failure_decision(self.failure_decision(&failure, &context), &failure);
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

        let delay = self.retry_delay(decision, attempts, hint);
        let context = context.with_next_delay(delay);
        if let Some(max_elapsed) = self.options.max_elapsed
            && will_exceed_elapsed(start.elapsed(), delay, max_elapsed)
        {
            return RetryFlowAction::Finished(RetryError::new(
                RetryErrorReason::MaxElapsedExceeded,
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
        } else if matches!(failure, AttemptFailure::Panic(_)) {
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

    /// Builds a max-elapsed error if the elapsed budget has already expired.
    ///
    /// # Parameters
    /// - `start`: Retry flow start.
    /// - `attempts`: Attempts executed so far.
    /// - `last_failure`: Last observed failure, if any.
    /// - `attempt_timeout`: Timeout visible in the terminal context.
    ///
    /// # Returns
    /// `Some(RetryError)` when the elapsed budget is exhausted.
    fn elapsed_error(
        &self,
        start: Instant,
        attempts: u32,
        last_failure: Option<AttemptFailure<E>>,
        attempt_timeout: Option<Duration>,
    ) -> Option<RetryError<E>> {
        let max_elapsed = self.options.max_elapsed?;
        let elapsed = start.elapsed();
        if elapsed < max_elapsed {
            return None;
        }
        Some(RetryError::new(
            RetryErrorReason::MaxElapsedExceeded,
            last_failure,
            self.context(start, attempts, Duration::ZERO, attempt_timeout),
        ))
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

/// Type-erased synchronous attempt used by the retry loop.
trait SyncAttempt<E> {
    /// Calls the wrapped operation once.
    ///
    /// # Returns
    /// `Ok(())` when the operation succeeded, or an attempt failure otherwise.
    fn call(&mut self) -> Result<(), AttemptFailure<E>>;
}

/// Adapter that stores the successful value outside the type-erased retry loop.
struct SyncValueOperation<T, F> {
    /// Wrapped caller operation.
    operation: F,
    /// Successful value produced by the operation.
    value: Option<T>,
}

impl<T, F> SyncValueOperation<T, F> {
    /// Creates a synchronous value-capturing operation adapter.
    ///
    /// # Parameters
    /// - `operation`: Operation to wrap.
    ///
    /// # Returns
    /// A new adapter with no captured value.
    fn new(operation: F) -> Self {
        Self {
            operation,
            value: None,
        }
    }

    /// Returns the value captured from a successful operation.
    ///
    /// # Returns
    /// The captured value.
    ///
    /// # Panics
    /// Panics only if the retry loop reports success without a successful
    /// operation result, which would indicate an internal logic error.
    fn into_value(self) -> T {
        self.value
            .expect("retry loop succeeded without an operation value")
    }
}

impl<T, E, F> SyncAttempt<E> for SyncValueOperation<T, F>
where
    F: FnMut() -> Result<T, E>,
{
    /// Calls the wrapped operation and stores successful values.
    ///
    /// # Returns
    /// `Ok(())` after storing a successful value, or an application failure.
    fn call(&mut self) -> Result<(), AttemptFailure<E>> {
        match (self.operation)() {
            Ok(value) => {
                self.value = Some(value);
                Ok(())
            }
            Err(error) => Err(AttemptFailure::Error(error)),
        }
    }
}

/// Boxed future returned by a value-erased async attempt.
#[cfg(feature = "tokio")]
type AsyncAttemptFuture<'a, E> = Pin<Box<dyn Future<Output = Result<(), AttemptFailure<E>>> + 'a>>;

/// Type-erased asynchronous attempt used by the retry loop.
#[cfg(feature = "tokio")]
trait AsyncAttempt<E> {
    /// Calls the wrapped async operation once.
    ///
    /// # Returns
    /// A future resolving to `Ok(())` on success or an attempt failure.
    fn call(&mut self) -> AsyncAttemptFuture<'_, E>;
}

/// Adapter that stores async operation success values outside the retry loop.
#[cfg(feature = "tokio")]
struct AsyncValueOperation<T, F> {
    /// Wrapped caller operation.
    operation: F,
    /// Successful value produced by the operation.
    value: Option<T>,
}

#[cfg(feature = "tokio")]
impl<T, F> AsyncValueOperation<T, F> {
    /// Creates an asynchronous value-capturing operation adapter.
    ///
    /// # Parameters
    /// - `operation`: Operation factory to wrap.
    ///
    /// # Returns
    /// A new adapter with no captured value.
    fn new(operation: F) -> Self {
        Self {
            operation,
            value: None,
        }
    }

    /// Returns the value captured from a successful async operation.
    ///
    /// # Returns
    /// The captured value.
    ///
    /// # Panics
    /// Panics only if the retry loop reports success without a successful
    /// operation result, which would indicate an internal logic error.
    fn into_value(self) -> T {
        self.value
            .expect("retry loop succeeded without an operation value")
    }
}

#[cfg(feature = "tokio")]
impl<T, E, F, Fut> AsyncAttempt<E> for AsyncValueOperation<T, F>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    /// Calls the wrapped async operation and stores successful values.
    ///
    /// # Returns
    /// A future resolving to `Ok(())` after storing a successful value, or an
    /// application failure.
    fn call(&mut self) -> AsyncAttemptFuture<'_, E> {
        Box::pin(async move {
            match (self.operation)().await {
                Ok(value) => {
                    self.value = Some(value);
                    Ok(())
                }
                Err(error) => Err(AttemptFailure::Error(error)),
            }
        })
    }
}

/// Internal control flow after a failed attempt.
enum RetryFlowAction<E> {
    /// Retry after `delay`.
    Retry {
        /// Delay before the next attempt.
        delay: Duration,
        /// Failure from the attempt that just completed.
        failure: AttemptFailure<E>,
    },
    /// Finish with a terminal error.
    Finished(RetryError<E>),
}

/// Message sent from one blocking attempt worker to the retry executor.
enum BlockingAttemptMessage<T, E> {
    /// Operation returned normally.
    Result(Result<T, E>),
    /// Operation panicked before timeout.
    Panic(AttemptPanic),
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

/// Checks whether sleeping would exhaust the elapsed-time budget.
///
/// # Parameters
/// - `elapsed`: Duration already consumed by the retry flow.
/// - `delay`: Delay before the next attempt.
/// - `max_elapsed`: Configured total elapsed-time budget.
///
/// # Returns
/// `true` when `elapsed + delay` reaches or exceeds `max_elapsed`, or when
/// duration addition overflows.
fn will_exceed_elapsed(elapsed: Duration, delay: Duration, max_elapsed: Duration) -> bool {
    elapsed
        .checked_add(delay)
        .is_none_or(|next_elapsed| next_elapsed >= max_elapsed)
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
