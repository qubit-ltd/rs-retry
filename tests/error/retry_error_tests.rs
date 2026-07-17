// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::error::Error;
use std::fmt;
use std::fmt::Write;
use std::sync::{
    Arc,
    Mutex,
    mpsc,
};
use std::thread;
use std::time::Duration;

use qubit_clock::{
    ManualMonotonicClock,
    MonotonicClock,
};
use qubit_retry::{
    AttemptFailure,
    AttemptFailureDecision,
    AttemptTimeoutOption,
    Retry,
    RetryContext,
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
    /// # Arguments
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
    /// # Arguments
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

    let elapsed_clock = ManualMonotonicClock::new_shared();
    let elapsed_sleeper = elapsed_clock.new_timer();
    let operation_clock = Arc::clone(&elapsed_clock);
    let elapsed_with_failure = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_operation_elapsed(Some(Duration::from_secs(5)))
        .no_delay()
        .blocking_timer(elapsed_sleeper)
        .build()
        .expect("retry should build")
        .run(move || -> Result<(), TestError> {
            operation_clock
                .advance(Duration::from_secs(10))
                .expect("manual time should advance");
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
        .expect_err(
            "zero total elapsed budget should stop before first attempt",
        );
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

    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker_still_running = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .attempt_timeout_option(Some(AttemptTimeoutOption::retry(
            Duration::from_millis(5),
        )))
        .worker_cancel_grace(Duration::from_millis(5))
        .build()
        .expect("retry should build")
        .run_in_worker(move |_token| {
            release_rx
                .lock()
                .expect("release receiver should be lockable")
                .recv()
                .expect("test should release the worker");
            finished_tx
                .send(())
                .expect("worker completion should be observable");
            Ok::<_, TestError>("late")
        })
        .expect_err("uncooperative worker should stop retries");
    release_tx
        .send(())
        .expect("timed-out worker should be releasable");
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("released worker should finish");
    assert_eq!(
        worker_still_running.to_string(),
        "retry worker still running after timeout cancellation grace, unreaped 1; last failure: attempt timed out"
    );
}

/// Verifies retry errors expose terminal failures as their source when
/// possible.
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
        .run_in_worker(|_token| -> Result<(), TestError> {
            panic!("panic source")
        })
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
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(5),
        )))
        .build()
        .expect("retry should build")
        .run_in_worker(|token| {
            while !token.is_cancelled() {
                thread::yield_now();
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

/// Verifies retry error display propagates formatter failures.
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

    let mut last_failure_writer =
        FailingWriter::fail_when_fragment_seen("; last failure:");
    assert!(write!(&mut last_failure_writer, "{attempts_exceeded}").is_err());
}
