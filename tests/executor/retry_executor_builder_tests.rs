/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{RetryDelay, RetryJitter, RetryExecutor, RetryOptions};

use crate::support::TestError;

/// Verifies builder validation reports option errors at build time.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when builder validation accepts invalid
/// options or reports the wrong key.
#[test]
fn test_build_validates_options_and_reports_builder_errors() {
    let options = RetryOptions::new(2, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid retry options should be created");
    let executor = RetryExecutor::<TestError>::from_options(options.clone())
        .expect("executor should be created from valid options");
    assert_eq!(executor.options(), &options);

    let invalid_delay = RetryExecutor::<TestError>::builder()
        .delay(RetryDelay::fixed(Duration::ZERO))
        .build()
        .expect_err("zero fixed delay should be rejected");
    assert_eq!(invalid_delay.path(), RetryOptions::KEY_DELAY);

    let invalid_jitter = RetryExecutor::<TestError>::builder()
        .jitter_factor(1.5)
        .build()
        .expect_err("out-of-range jitter should be rejected");
    assert_eq!(invalid_jitter.path(), RetryOptions::KEY_JITTER_FACTOR);

    let invalid_attempts = RetryExecutor::<TestError>::builder()
        .max_attempts(0)
        .build()
        .expect_err("zero attempts should be rejected");
    assert_eq!(invalid_attempts.path(), RetryOptions::KEY_MAX_ATTEMPTS);
}

/// Verifies the default builder and executor debug output.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when default construction or debug output
/// is incorrect.
#[test]
fn test_default_and_debug_work() {
    let executor = qubit_retry::RetryExecutorBuilder::<TestError>::default()
        .delay(RetryDelay::none())
        .build()
        .expect("default builder should create an executor");

    assert!(format!("{executor:?}").contains("RetryExecutor"));
}

/// Verifies default-classifier construction supports non-static error inputs.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when `from_options` or `build` does not
/// accept borrowed (non-`'static`) error types.
#[test]
fn test_build_and_from_options_allow_borrowed_error_type() {
    #[derive(Debug, PartialEq, Eq)]
    struct BorrowedError<'a>(&'a str);

    let message = String::from("borrowed");
    let options = RetryOptions::new(2, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid retry options should be created");

    let from_options_executor = RetryExecutor::<BorrowedError<'_>>::from_options(options.clone())
        .expect("executor from options should support borrowed error types");
    assert_eq!(from_options_executor.options(), &options);

    let built_executor = RetryExecutor::<BorrowedError<'_>>::builder()
        .delay(RetryDelay::none())
        .build()
        .expect("builder should support borrowed error types");
    assert_eq!(built_executor.options().max_attempts.get(), 3);

    let run_result = from_options_executor
        .run(|| -> Result<(), BorrowedError<'_>> { Err(BorrowedError(message.as_str())) });
    let run_error = run_result.expect_err("operation should fail with borrowed error");
    assert_eq!(run_error.attempts(), 2);
    assert_eq!(run_error.last_error(), Some(&BorrowedError("borrowed")));
}
