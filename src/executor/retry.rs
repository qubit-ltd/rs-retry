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
#[cfg(feature = "tokio")]
use std::pin::Pin;
use std::time::{Duration, Instant};

use qubit_common::BoxError;
use qubit_function::{BiConsumer, BiFunction, Consumer};

use crate::event::RetryListeners;
use crate::{
    AttemptFailure, AttemptFailureDecision, RetryAfterHint, RetryBuilder, RetryConfigError,
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
    /// Optional timeout for async attempts.
    attempt_timeout: Option<Duration>,
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
            if let Some(error) =
                self.elapsed_error(start, attempts, last_failure.take(), self.attempt_timeout)
            {
                return Err(self.emit_error(error));
            }

            attempts += 1;
            let before_context =
                self.context(start, attempts, Duration::ZERO, self.attempt_timeout);
            self.emit_before_attempt(&before_context);

            let attempt_start = Instant::now();
            let result = if let Some(timeout) = self.attempt_timeout {
                match tokio::time::timeout(timeout, operation.call()).await {
                    Ok(result) => result,
                    Err(_) => Err(AttemptFailure::Timeout),
                }
            } else {
                operation.call().await
            };

            let context = self.context(
                start,
                attempts,
                attempt_start.elapsed(),
                self.attempt_timeout,
            );
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
    /// - `attempt_timeout`: Optional async attempt timeout.
    /// - `retry_after_hint`: Optional hint extractor.
    /// - `isolate_listener_panics`: Whether listener panics are isolated.
    /// - `listeners`: Lifecycle listeners.
    ///
    /// # Returns
    /// A retry policy.
    pub(super) fn new(
        options: RetryOptions,
        attempt_timeout: Option<Duration>,
        retry_after_hint: Option<RetryAfterHint<E>>,
        isolate_listener_panics: bool,
        listeners: RetryListeners<E>,
    ) -> Self {
        Self {
            options,
            attempt_timeout,
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

        let decision = self.failure_decision(&failure, &context);
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
            .field("attempt_timeout", &self.attempt_timeout)
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
