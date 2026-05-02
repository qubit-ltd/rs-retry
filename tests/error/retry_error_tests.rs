/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::error::Error;
use std::fmt;
use std::fmt::Write;
use std::thread;
use std::time::Duration;

#[cfg(coverage)]
use qubit_retry::{AttemptExecutorError, RetryError};
use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, AttemptTimeoutOption, Retry, RetryContext,
    RetryErrorReason,
};

use crate::support::TestError;

/// Test writer that can force formatter failures at controlled points.
struct FailingWriter {
    fail_on_first_write: bool,
    fail_when_fragment_seen: Option<&'static str>,
}

impl FailingWriter {
    /// Creates a writer that fails immediately.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A writer whose first write returns [`fmt::Error`].
    fn fail_immediately() -> Self {
        Self {
            fail_on_first_write: true,
            fail_when_fragment_seen: None,
        }
    }

    /// Creates a writer that fails when a fragment appears.
    ///
    /// # Parameters
    /// - `fragment`: Text fragment that triggers [`fmt::Error`].
    ///
    /// # Returns
    /// A writer that succeeds until a write contains `fragment`.
    fn fail_when_fragment_seen(fragment: &'static str) -> Self {
        Self {
            fail_on_first_write: false,
            fail_when_fragment_seen: Some(fragment),
        }
    }
}

impl fmt::Write for FailingWriter {
    /// Writes a string or returns a configured formatting error.
    ///
    /// # Parameters
    /// - `s`: Text fragment emitted by the formatter.
    ///
    /// # Returns
    /// `Ok(())` unless this writer is configured to fail for the current write.
    ///
    /// # Errors
    /// Returns [`fmt::Error`] for the configured failure point.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.fail_on_first_write
            || self
                .fail_when_fragment_seen
                .is_some_and(|fragment| s.contains(fragment))
        {
            return Err(fmt::Error);
        }
        Ok(())
    }
}

/// Verifies retry errors preserve terminal reason, context, and last failure.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_error_preserves_reason_context_and_last_failure() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("single failing attempt should stop");

    assert_eq!(error.reason(), RetryErrorReason::AttemptsExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(error.context().max_attempts(), 1);
    assert_eq!(error.last_error(), Some(&TestError("failed")));
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Error(TestError("failed")))
    ));
    assert_eq!(error.into_last_error(), Some(TestError("failed")));
}

/// Verifies `into_parts()` returns complete terminal retry data.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_retry_error_into_parts_returns_reason_failure_and_context() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("parts")) })
        .expect_err("single failing attempt should stop");
    let (reason, last_failure, context) = error.into_parts();

    assert_eq!(reason, RetryErrorReason::AttemptsExceeded);
    assert!(matches!(
        last_failure,
        Some(AttemptFailure::Error(TestError("parts")))
    ));
    assert_eq!(context.attempt(), 1);
    assert_eq!(context.max_attempts(), 1);
}

/// Verifies retry error display output covers all terminal reasons.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when display output changes unexpectedly.
#[test]
fn test_retry_error_display_formats_terminal_reasons() {
    let aborted = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                AttemptFailureDecision::Abort
            },
        )
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("fatal")) })
        .expect_err("failure listener should abort");
    assert_eq!(
        aborted.to_string(),
        "retry aborted after 1 attempt(s); last failure: fatal"
    );

    let attempts_exceeded = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("single failed attempt should exceed attempts");
    assert_eq!(
        attempts_exceeded.to_string(),
        "retry attempts exceeded after 1 attempt(s), max 1; last failure: failed"
    );

    let elapsed_with_failure = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_millis(5)))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> {
            std::thread::sleep(Duration::from_millis(10));
            Err(TestError("slow"))
        })
        .expect_err("operation execution should exceed elapsed budget");
    assert_eq!(
        elapsed_with_failure.to_string(),
        "retry max operation elapsed exceeded after 1 attempt(s); last failure: slow"
    );

    let elapsed_without_failure = Retry::<TestError>::builder()
        .max_operation_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");
    assert_eq!(
        elapsed_without_failure.to_string(),
        "retry max operation elapsed exceeded after 0 attempt(s)"
    );

    let total_elapsed_without_failure = Retry::<TestError>::builder()
        .max_total_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero total elapsed budget should stop before first attempt");
    assert_eq!(
        total_elapsed_without_failure.to_string(),
        "retry max total elapsed exceeded after 0 attempt(s)"
    );

    let unsupported = Retry::<TestError>::builder()
        .max_attempts(3)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .no_delay()
        .build()
        .expect("retry should build")
        .run::<(), _>(|| Ok::<_, TestError>(()))
        .expect_err("run() should reject attempt_timeout");
    assert_eq!(
        unsupported.to_string(),
        "run() does not support attempt timeout; use run_async() or run_in_worker()"
    );
    assert_eq!(
        unsupported.attempt_timeout_source(),
        Some(qubit_retry::AttemptTimeoutSource::Configured)
    );

    let worker_still_running = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::retry(Duration::from_millis(5))))
        .worker_cancel_grace(Duration::from_millis(5))
        .build()
        .expect("retry should build")
        .run_in_worker(|_token| {
            thread::sleep(Duration::from_millis(120));
            Ok::<_, TestError>("late")
        })
        .expect_err("uncooperative worker should stop retries");
    assert_eq!(
        worker_still_running.to_string(),
        "retry worker still running after timeout cancellation grace, unreaped 1; last failure: attempt timed out"
    );
}

/// Verifies retry errors expose terminal failures as their source when possible.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when source propagation is incorrect.
#[test]
fn test_retry_error_source_returns_terminal_failure() {
    let with_source = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("source")) })
        .expect_err("single failed attempt should exceed attempts");
    assert_eq!(
        with_source
            .source()
            .expect("last application error should be the source")
            .to_string(),
        "source"
    );

    let panic_source = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build")
        .run_in_worker(|_token| -> Result<(), TestError> { panic!("panic source") })
        .expect_err("worker panic should abort");
    assert_eq!(
        panic_source
            .source()
            .expect("captured panic should be the source")
            .to_string(),
        "panic source"
    );

    let without_source = Retry::<TestError>::builder()
        .max_operation_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");
    assert!(without_source.source().is_none());

    let timeout_error = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(Duration::from_millis(5))))
        .build()
        .expect("retry should build")
        .run_blocking_with_timeout(|token| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
            Err::<(), TestError>(TestError("cancelled too late"))
        })
        .expect_err("attempt timeout should abort");
    assert!(matches!(
        timeout_error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
    assert!(timeout_error.source().is_none());
    assert!(timeout_error.last_error().is_none());
    assert!(timeout_error.into_last_error().is_none());
}

/// Verifies coverage-only construction can exercise executor source handling.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when source propagation is incorrect.
#[test]
#[cfg(coverage)]
fn test_retry_error_coverage_constructor_exposes_executor_source() {
    let error = RetryError::coverage_new(
        RetryErrorReason::Aborted,
        Some(AttemptFailure::<TestError>::Executor(
            AttemptExecutorError::new("executor source"),
        )),
        RetryContext::new(1, 1),
    );

    assert_eq!(
        error
            .source()
            .expect("executor error should be the source")
            .to_string(),
        "executor source"
    );
}

/// Verifies retry error display propagates formatter failures.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when display formatting swallows write errors.
#[test]
fn test_retry_error_display_propagates_formatter_errors() {
    let aborted = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                AttemptFailureDecision::Abort
            },
        )
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("fatal")) })
        .expect_err("failure listener should abort");
    let attempts_exceeded = Retry::<TestError>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("failed")) })
        .expect_err("single failed attempt should exceed attempts");
    let max_operation_elapsed = Retry::<TestError>::builder()
        .max_operation_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");

    let mut aborted_writer = FailingWriter::fail_immediately();
    assert!(write!(&mut aborted_writer, "{aborted}").is_err());

    let mut attempts_writer = FailingWriter::fail_immediately();
    assert!(write!(&mut attempts_writer, "{attempts_exceeded}").is_err());

    let mut elapsed_writer = FailingWriter::fail_immediately();
    assert!(write!(&mut elapsed_writer, "{max_operation_elapsed}").is_err());

    let mut last_failure_writer = FailingWriter::fail_when_fragment_seen("; last failure:");
    assert!(write!(&mut last_failure_writer, "{attempts_exceeded}").is_err());
}
