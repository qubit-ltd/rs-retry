/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry executor execution.
//!
//! A [`RetryExecutor`] owns validated retry options, a retry decider, and
//! optional listeners. It can execute synchronous operations or asynchronous
//! operations on Tokio.

use std::fmt;
use std::time::{Duration, Instant};
#[cfg(feature = "async-tokio")]
use std::future::Future;

use qubit_common::BoxError;
use qubit_function::{BiConsumer, BiFunction, Consumer};

use crate::event::RetryListeners;
use crate::{
    RetryAbortContext, RetryAttemptContext, RetryAttemptFailure, RetryConfigError, RetryContext,
    RetryDecision, RetryError, RetryFailureContext, RetryOptions, RetrySuccessContext,
};

use super::retry_executor_builder::RetryExecutorBuilder;
use crate::error::RetryDecider;
use crate::error::RetryFailureAction;

/// Retry executor bound to an error type.
///
/// The generic parameter `E` is the caller's application error type. The
/// success type is chosen per call to [`RetryExecutor::run`],
/// [`RetryExecutor::run_async`], or [`RetryExecutor::run_async_with_timeout`].
/// Cloning an executor shares the retry decider and listeners through reference
/// counting.
#[derive(Clone)]
pub struct RetryExecutor<E = BoxError> {
    /// Validated limits and backoff settings (`max_attempts`, delay strategy,
    /// jitter, optional `max_elapsed`).
    options: RetryOptions,
    /// Decides whether to retry after each application error; timeouts from
    /// [`RetryAttemptFailure::AttemptTimeout`] use `timeout_decision`.
    retry_decider: RetryDecider<E>,
    /// Decision used when one attempt fails with [`RetryAttemptFailure::AttemptTimeout`].
    timeout_decision: RetryDecision,
    /// Optional hooks invoked on success, retry scheduling, terminal failure,
    /// or decider-initiated abort.
    listeners: RetryListeners<E>,
}

impl<E> RetryExecutor<E> {
    /// Creates a retry executor builder.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A [`RetryExecutorBuilder`] configured with default options and the default
    /// retry-all decider.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    pub fn builder() -> RetryExecutorBuilder<E> {
        RetryExecutorBuilder::new()
    }

    /// Creates an executor from options with the default decider.
    ///
    /// # Parameters
    /// - `options`: Validated retry options to use for the executor.
    ///
    /// # Returns
    /// A [`RetryExecutor`] that retries all application errors unless limits stop
    /// execution.
    ///
    /// # Errors
    /// Returns [`RetryConfigError`] if `options` fails validation.
    pub fn from_options(options: RetryOptions) -> Result<Self, RetryConfigError> {
        Self::builder().options(options).build()
    }

    /// Returns the immutable option snapshot used by this executor.
    ///
    /// # Parameters
    /// This method has no parameters.
    ///
    /// # Returns
    /// A shared reference to the executor's retry options.
    ///
    /// # Errors
    /// This method does not return errors.
    #[inline]
    pub fn options(&self) -> &RetryOptions {
        &self.options
    }

    /// Runs a synchronous operation with retry.
    ///
    /// Loops under global elapsed-time and per-executor attempt limits. Before
    /// each try, exits with an error if the wall-clock budget is already spent
    /// (without calling `operation` again). Success emits listener events and
    /// returns `Ok(T)`. Failure goes through `handle_failure`: either
    /// `std::thread::sleep` then retry, or return a terminal [`RetryError`].
    ///
    /// # Parameters
    /// - `operation`: Synchronous operation to execute. It is called once per
    ///   attempt until it returns `Ok`, the decider aborts, or retry limits
    ///   are exhausted.
    ///
    /// # Returns
    /// `Ok(T)` with the operation result, or [`RetryError`] preserving the last
    /// application error or timeout metadata.
    ///
    /// # Errors
    /// Returns [`RetryError::Aborted`] when the decider aborts,
    /// [`RetryError::AttemptsExceeded`] when the attempt limit is reached, or
    /// [`RetryError::MaxElapsedExceeded`] when the total elapsed-time budget is
    /// exhausted.
    ///
    /// # Panics
    /// Propagates any panic raised by `operation` or by registered listeners.
    ///
    /// # Blocking
    /// This method blocks the current thread with `std::thread::sleep` between
    /// retry attempts when the computed delay is non-zero.
    pub fn run<T, F>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            // Before each attempt: if total wall-clock budget is already spent,
            // stop without calling `operation` again (consumes `last_failure`
            // for the terminal error).
            if let Some(error) = self.take_elapsed_error(start, attempts, &mut last_failure) {
                return Err(error);
            }

            attempts += 1;
            match operation() {
                Ok(value) => {
                    self.emit_success(attempts, start.elapsed());
                    return Ok(value);
                }
                Err(error) => {
                    let failure = RetryAttemptFailure::Error(error);
                    // Decider + limits decide whether to sleep and retry or
                    // finish with a terminal `RetryError`.
                    match self.handle_failure(attempts, start, failure) {
                        RetryFailureAction::Retry { delay, failure } => {
                            if !delay.is_zero() {
                                std::thread::sleep(delay);
                            }
                            last_failure = Some(failure);
                        }
                        RetryFailureAction::Finished(error) => return Err(error),
                    }
                }
            }
        }
    }

    /// Runs an asynchronous operation with retry.
    ///
    /// Same loop structure as [`Self::run`]: checks the global elapsed budget
    /// before each attempt, increments the attempt counter, then runs
    /// `operation().await`. On failure, `handle_failure` chooses a backoff
    /// `sleep` (async) or a terminal [`RetryError`]; timing uses Tokio's timer
    /// instead of blocking `std::thread::sleep`.
    ///
    /// # Parameters
    /// - `operation`: Asynchronous operation factory. It is called once per
    ///   attempt and must return a fresh future each time.
    ///
    /// # Returns
    /// `Ok(T)` with the operation result, or [`RetryError`] preserving the last
    /// application error.
    ///
    /// # Errors
    /// Returns [`RetryError::Aborted`] when the decider aborts,
    /// [`RetryError::AttemptsExceeded`] when the attempt limit is reached, or
    /// [`RetryError::MaxElapsedExceeded`] when the total elapsed-time budget is
    /// exhausted.
    ///
    /// # Panics
    /// Propagates panics raised by `operation`, the returned future, or
    /// registered listeners. Tokio may also panic if the future is polled
    /// outside a runtime with a time driver and a non-zero sleep is required.
    ///
    /// # Async
    /// Uses `tokio::time::sleep` between attempts when the computed delay is
    /// non-zero.
    #[cfg(feature = "async-tokio")]
    pub async fn run_async<T, F, Fut>(&self, mut operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            // Same elapsed-budget gate as `run`: avoid scheduling another
            // attempt once total time is exhausted.
            if let Some(error) = self.take_elapsed_error(start, attempts, &mut last_failure) {
                return Err(error);
            }

            attempts += 1;
            match operation().await {
                Ok(value) => {
                    self.emit_success(attempts, start.elapsed());
                    return Ok(value);
                }
                Err(error) => {
                    let failure = RetryAttemptFailure::Error(error);
                    // Decider + limits decide whether to sleep and retry or
                    // finish with a terminal `RetryError`.
                    match self.handle_failure(attempts, start, failure) {
                        RetryFailureAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            last_failure = Some(failure);
                        }
                        RetryFailureAction::Finished(error) => return Err(error),
                    }
                }
            }
        }
    }

    /// Runs an asynchronous operation with a timeout for each attempt.
    ///
    /// Like [`Self::run_async`], but each attempt is bounded by
    /// `tokio::time::timeout(attempt_timeout, operation())`. Normal completion
    /// yields the inner `Ok`/`Err`. A timeout becomes
    /// [`RetryAttemptFailure::AttemptTimeout`] and is fed to `handle_failure` to
    /// decide retry versus a terminal error, subject to the same global limits.
    ///
    /// # Parameters
    /// - `attempt_timeout`: Maximum duration allowed for each individual
    ///   attempt.
    /// - `operation`: Asynchronous operation factory. It is called once per
    ///   attempt and must return a fresh future each time.
    ///
    /// # Returns
    /// `Ok(T)` with the operation result, or [`RetryError`] preserving the last
    /// application error or timeout metadata.
    ///
    /// # Errors
    /// Returns [`RetryError::Aborted`] when the decider aborts an
    /// application error, [`RetryError::AttemptsExceeded`] when the attempt
    /// limit is reached, or [`RetryError::MaxElapsedExceeded`] when the total
    /// elapsed-time budget is exhausted. Attempt timeouts are represented as
    /// [`RetryAttemptFailure::AttemptTimeout`] and are considered retryable until
    /// limits stop execution.
    ///
    /// # Panics
    /// Propagates panics raised by `operation`, the returned future, or
    /// registered listeners. Tokio may also panic if the future is polled
    /// outside a runtime with a time driver.
    ///
    /// # Async
    /// Uses `tokio::time::timeout` for each attempt and `tokio::time::sleep`
    /// between retries when the computed delay is non-zero.
    #[cfg(feature = "async-tokio")]
    pub async fn run_async_with_timeout<T, F, Fut>(
        &self,
        attempt_timeout: Duration,
        mut operation: F,
    ) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let mut attempts = 0;
        let mut last_failure = None;

        loop {
            // Total elapsed budget check before starting the timed attempt.
            if let Some(error) = self.take_elapsed_error(start, attempts, &mut last_failure) {
                return Err(error);
            }

            attempts += 1;
            let attempt_start = Instant::now();
            // Outer `Result` is from `tokio::time::timeout`; inner is the
            // operation's `Result<T, E>`.
            match tokio::time::timeout(attempt_timeout, operation()).await {
                Ok(Ok(value)) => {
                    self.emit_success(attempts, start.elapsed());
                    return Ok(value);
                }
                Ok(Err(error)) => {
                    let failure = RetryAttemptFailure::Error(error);
                    match self.handle_failure(attempts, start, failure) {
                        RetryFailureAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            last_failure = Some(failure);
                        }
                        RetryFailureAction::Finished(error) => return Err(error),
                    }
                }
                Err(_) => {
                    // Per-attempt budget exceeded before the future completed.
                    let failure = RetryAttemptFailure::AttemptTimeout {
                        elapsed: attempt_start.elapsed(),
                        timeout: attempt_timeout,
                    };
                    match self.handle_failure(attempts, start, failure) {
                        RetryFailureAction::Retry { delay, failure } => {
                            sleep_async(delay).await;
                            last_failure = Some(failure);
                        }
                        RetryFailureAction::Finished(error) => return Err(error),
                    }
                }
            }
        }
    }

    /// Creates an executor from already validated parts.
    ///
    /// # Parameters
    /// - `options`: Retry options used by the executor.
    /// - `retry_decider`: [`RetryDecider`] shared by cloned executors (uses each application error `E`).
    /// - `timeout_decision`: Decision used for [`RetryAttemptFailure::AttemptTimeout`].
    /// - `listeners`: Optional callbacks invoked during execution.
    ///
    /// # Returns
    /// A new [`RetryExecutor`].
    ///
    /// # Errors
    /// This function does not return errors. Callers must validate `options`
    /// before constructing the executor.
    pub(super) fn new(
        options: RetryOptions,
        retry_decider: RetryDecider<E>,
        timeout_decision: RetryDecision,
        listeners: RetryListeners<E>,
    ) -> Self {
        Self {
            options,
            retry_decider,
            timeout_decision,
            listeners,
        }
    }

    /// Handles a failed attempt and decides the next executor action.
    ///
    /// # Processing
    /// 1. Builds [`RetryAttemptContext`] from elapsed time and attempt counts, then
    ///    asks the [`RetryDecider`]: application errors `E` go through it;
    ///    [`RetryAttemptFailure::AttemptTimeout`] is treated as retryable unless
    ///    limits below apply.
    /// 2. If the decision is [`RetryDecision::Abort`], emits the abort listener
    ///    and returns [`RetryFailureAction::Finished`] with [`RetryError::Aborted`].
    /// 3. If `attempts` has reached `max_attempts`, emits the failure listener
    ///    and returns [`RetryFailureAction::Finished`] with
    ///    [`RetryError::AttemptsExceeded`].
    /// 4. Otherwise computes base delay for this attempt, applies jitter, and if
    ///    `max_elapsed` is set and sleeping would exceed the total time budget,
    ///    emits the failure listener and returns [`RetryFailureAction::Finished`]
    ///    with [`RetryError::MaxElapsedExceeded`].
    /// 5. Otherwise emits the retry listener and returns [`RetryFailureAction::Retry`]
    ///    with the computed delay and the same failure payload.
    ///
    /// # Parameters
    /// - `attempts`: Number of attempts executed so far.
    /// - `start`: Start time of the whole retry execution.
    /// - `failure`: Failure produced by the latest attempt.
    ///
    /// # Returns
    /// [`RetryFailureAction::Retry`] with the computed delay when retrying should
    /// continue, or [`RetryFailureAction::Finished`] with the terminal
    /// [`RetryError`].
    ///
    /// # Errors
    /// This function does not return errors directly; terminal retry errors are
    /// returned inside [`RetryFailureAction::Finished`].
    ///
    /// # Side Effects
    /// Invokes retry, failure, or abort listeners depending on the decision.
    fn handle_failure(
        &self,
        attempts: u32,
        start: Instant,
        failure: RetryAttemptFailure<E>,
    ) -> RetryFailureAction<E> {
        let elapsed = start.elapsed();
        let context = RetryAttemptContext {
            attempt: attempts,
            max_attempts: self.options.max_attempts.get(),
            elapsed,
        };
        // Application errors consult the decider; attempt timeouts use the
        // builder-configured timeout decision.
        let decision = match &failure {
            RetryAttemptFailure::Error(error) => self.retry_decider.apply(error, &context),
            RetryAttemptFailure::AttemptTimeout { .. } => self.timeout_decision,
        };
        if decision == RetryDecision::Abort {
            self.emit_abort(attempts, elapsed, &failure);
            return RetryFailureAction::Finished(RetryError::Aborted {
                attempts,
                elapsed,
                failure,
            });
        }

        // `attempts` is the count including this failure; reaching
        // `max_attempts` means no further attempts remain.
        let max_attempts = self.options.max_attempts.get();
        if attempts >= max_attempts {
            let Some(failure) = self.emit_failure(attempts, elapsed, Some(failure)) else {
                unreachable!("failure must exist when attempts exceed max_attempts");
            };
            return RetryFailureAction::Finished(RetryError::AttemptsExceeded {
                attempts,
                max_attempts,
                elapsed,
                last_failure: failure,
            });
        }

        let delay = self
            .options
            .jitter
            .delay_for_attempt(&self.options.delay, attempts);
        // If sleeping would push total elapsed past the budget, finish now
        // instead of sleeping once and failing on the next loop iteration.
        if let Some(max_elapsed) = self.options.max_elapsed {
            if will_exceed_elapsed(start.elapsed(), delay, max_elapsed) {
                let last_failure = self.emit_failure(attempts, elapsed, Some(failure));
                let error = RetryError::MaxElapsedExceeded {
                    attempts,
                    elapsed,
                    max_elapsed,
                    last_failure,
                };
                return RetryFailureAction::Finished(error);
            }
        }

        self.emit_retry(attempts, elapsed, delay, &failure);
        RetryFailureAction::Retry { delay, failure }
    }

    /// Checks whether the total elapsed-time budget has already been reached.
    ///
    /// # Parameters
    /// - `start`: Start time of the whole retry execution.
    /// - `attempts`: Number of attempts executed so far.
    /// - `last_failure`: Mutable slot containing the last failure, consumed
    ///   when the elapsed budget is exceeded.
    ///
    /// # Returns
    /// `Some(RetryError)` when the elapsed budget is exhausted; otherwise
    /// `None`.
    ///
    /// # Errors
    /// This function does not return errors directly; the terminal error is
    /// returned as `Some`.
    ///
    /// # Side Effects
    /// Emits a failure event when the elapsed budget is exhausted.
    fn take_elapsed_error(
        &self,
        start: Instant,
        attempts: u32,
        last_failure: &mut Option<RetryAttemptFailure<E>>,
    ) -> Option<RetryError<E>> {
        let max_elapsed = self.options.max_elapsed?;
        let elapsed = start.elapsed();
        // Still within budget: allow another attempt (and a possible inter-attempt
        // sleep).
        if elapsed < max_elapsed {
            return None;
        }
        // Budget already consumed: do not start another attempt; attach the
        // last failure if we have one from the previous attempt.
        let last_failure = self.emit_failure(attempts, elapsed, last_failure.take());
        Some(RetryError::MaxElapsedExceeded {
            attempts,
            elapsed,
            max_elapsed,
            last_failure,
        })
    }

    /// Emits the retry listener event when a listener is registered.
    ///
    /// # Parameters
    /// - `attempt`: Attempt that just failed.
    /// - `elapsed`: Total elapsed duration before sleeping.
    /// - `next_delay`: RetryDelay that will be slept before the next attempt.
    /// - `failure`: Failure that triggered the retry.
    ///
    /// # Returns
    /// This function returns nothing.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// Propagates any panic raised by the listener.
    fn emit_retry(
        &self,
        attempt: u32,
        elapsed: Duration,
        next_delay: Duration,
        failure: &RetryAttemptFailure<E>,
    ) {
        if let Some(listener) = &self.listeners.retry {
            listener.accept(
                &RetryContext {
                    attempt,
                    max_attempts: self.options.max_attempts.get(),
                    elapsed,
                    next_delay,
                },
                failure,
            );
        }
    }

    /// Emits the success listener event when a listener is registered.
    ///
    /// # Parameters
    /// - `attempts`: Number of attempts that were executed.
    /// - `elapsed`: Total elapsed duration at success.
    ///
    /// # Returns
    /// This function returns nothing.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// Propagates any panic raised by the listener.
    fn emit_success(&self, attempts: u32, elapsed: Duration) {
        if let Some(listener) = &self.listeners.success {
            listener.accept(&RetrySuccessContext { attempts, elapsed });
        }
    }

    /// Emits the failure listener event when a listener is registered.
    ///
    /// # Parameters
    /// - `attempts`: Number of attempts that were executed.
    /// - `elapsed`: Total elapsed duration at failure.
    /// - `failure`: Final failure, or `None` if no attempt ran.
    ///
    /// # Returns
    /// This function returns nothing.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// Propagates any panic raised by the listener.
    fn emit_failure(
        &self,
        attempts: u32,
        elapsed: Duration,
        failure: Option<RetryAttemptFailure<E>>,
    ) -> Option<RetryAttemptFailure<E>> {
        if let Some(listener) = &self.listeners.failure {
            listener.accept(&RetryFailureContext { attempts, elapsed }, &failure);
        }
        failure
    }

    /// Emits the abort listener event when a listener is registered.
    ///
    /// # Parameters
    /// - `attempts`: Number of attempts that were executed.
    /// - `elapsed`: Total elapsed duration at abort.
    /// - `failure`: Failure that caused the decider to abort retrying.
    ///
    /// # Returns
    /// This function returns nothing.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// Propagates any panic raised by the listener.
    fn emit_abort(&self, attempts: u32, elapsed: Duration, failure: &RetryAttemptFailure<E>) {
        if let Some(listener) = &self.listeners.abort {
            listener.accept(&RetryAbortContext { attempts, elapsed }, failure);
        }
    }
}

impl<E> fmt::Debug for RetryExecutor<E> {
    /// Formats the executor for debug output without exposing callbacks.
    ///
    /// # Parameters
    /// - `f`: Formatter provided by the standard formatting machinery.
    ///
    /// # Returns
    /// `fmt::Result` from the formatter.
    ///
    /// # Errors
    /// Returns a formatting error if the underlying formatter fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryExecutor")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// Checks whether sleeping would exhaust the elapsed-time budget.
///
/// # Parameters
/// - `elapsed`: Duration already consumed by the retry execution.
/// - `delay`: RetryDelay that would be slept before the next attempt.
/// - `max_elapsed`: Configured total elapsed-time budget.
///
/// # Returns
/// `true` when `elapsed + delay` is greater than or equal to `max_elapsed`, or
/// when duration addition overflows.
///
/// # Errors
/// This function does not return errors.
fn will_exceed_elapsed(elapsed: Duration, delay: Duration, max_elapsed: Duration) -> bool {
    // Treat overflow as "would exceed" so we never sleep with a bogus huge duration.
    elapsed
        .checked_add(delay)
        .map_or(true, |next_elapsed| next_elapsed >= max_elapsed)
}

/// Sleeps asynchronously when the delay is non-zero.
///
/// # Parameters
/// - `delay`: Duration to sleep.
///
/// # Returns
/// This function returns after the sleep completes, or immediately for a zero
/// delay.
///
/// # Errors
/// This function does not return errors.
///
/// # Panics
/// Tokio may panic if this future is polled outside a runtime with a time
/// driver and `delay` is non-zero.
#[cfg(feature = "async-tokio")]
async fn sleep_async(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies elapsed-budget checks handle below-bound, at-bound, and
    /// overflowing durations.
    ///
    /// # Parameters
    /// This test has no parameters.
    ///
    /// # Returns
    /// This test returns nothing.
    ///
    /// # Errors
    /// The test fails through assertions when elapsed-budget checks return the
    /// wrong decision.
    #[test]
    fn test_will_exceed_elapsed_handles_boundaries_and_overflow() {
        let one_ms = Duration::from_millis(1);
        let two_ms = Duration::from_millis(2);

        assert!(!will_exceed_elapsed(one_ms, Duration::ZERO, two_ms));
        assert!(will_exceed_elapsed(one_ms, one_ms, two_ms));
        assert!(will_exceed_elapsed(Duration::MAX, one_ms, Duration::MAX));
    }

    /// Verifies async sleep returns for zero and non-zero delays.
    ///
    /// # Parameters
    /// This test has no parameters.
    ///
    /// # Returns
    /// This test returns nothing.
    ///
    /// # Errors
    /// The test fails if Tokio's timer does not complete either sleep.
    #[cfg(feature = "async-tokio")]
    #[tokio::test]
    async fn test_sleep_async_handles_zero_and_nonzero_delays() {
        sleep_async(Duration::ZERO).await;
        sleep_async(Duration::from_millis(1)).await;
    }

    /// Verifies the default boxed-error executor type runs success and failure
    /// paths.
    ///
    /// # Parameters
    /// This test has no parameters.
    ///
    /// # Returns
    /// This test returns nothing.
    ///
    /// # Errors
    /// The test fails through assertions when default boxed-error execution
    /// returns incorrect metadata.
    #[test]
    fn test_box_error_executor_runs_success_and_failure_paths() {
        let executor: RetryExecutor<BoxError> = RetryExecutor::builder()
            .max_attempts(1)
            .delay(crate::RetryDelay::none())
            .build()
            .expect("executor should be built");

        assert_eq!(executor.options().max_attempts(), 1);
        assert!(format!("{executor:?}").contains("RetryExecutor"));
        let value = executor
            .run(|| Ok::<_, BoxError>("default-box-error"))
            .expect("boxed-error executor should return success");
        assert_eq!(value, "default-box-error");

        let error = executor
            .run(|| -> Result<(), BoxError> {
                let source = std::io::Error::new(std::io::ErrorKind::Other, "boxed failure");
                Err(Box::new(source) as BoxError)
            })
            .expect_err("single failed attempt should exceed attempts");

        assert_eq!(error.attempts(), 1);
        assert_eq!(
            error.last_error().map(ToString::to_string).as_deref(),
            Some("boxed failure")
        );
    }
}
