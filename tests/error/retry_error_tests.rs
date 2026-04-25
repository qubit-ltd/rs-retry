/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::error::Error;
use std::fmt;
use std::fmt::Write;
use std::time::Duration;

use qubit_retry::{AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryErrorReason};

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
        .max_elapsed(Some(Duration::from_millis(5)))
        .fixed_delay(Duration::from_millis(5))
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { Err(TestError("slow")) })
        .expect_err("retry delay should exceed elapsed budget");
    assert_eq!(
        elapsed_with_failure.to_string(),
        "retry max elapsed exceeded after 1 attempt(s); last failure: slow"
    );

    let elapsed_without_failure = Retry::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");
    assert_eq!(
        elapsed_without_failure.to_string(),
        "retry max elapsed exceeded after 0 attempt(s)"
    );
}

/// Verifies retry errors expose the last application error as their source.
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
fn test_retry_error_source_returns_last_application_error() {
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

    let without_source = Retry::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
        .no_delay()
        .build()
        .expect("retry should build")
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("zero elapsed budget should stop before first attempt");
    assert!(without_source.source().is_none());
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
    let max_elapsed = Retry::<TestError>::builder()
        .max_elapsed(Some(Duration::ZERO))
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
    assert!(write!(&mut elapsed_writer, "{max_elapsed}").is_err());

    let mut last_failure_writer = FailingWriter::fail_when_fragment_seen("; last failure:");
    assert!(write!(&mut last_failure_writer, "{attempts_exceeded}").is_err());
}
