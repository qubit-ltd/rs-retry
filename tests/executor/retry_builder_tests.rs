/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::constants::DEFAULT_RETRY_MAX_ATTEMPTS;
use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, AttemptTimeoutOption, AttemptTimeoutPolicy, Retry,
    RetryDelay, RetryJitter, RetryOptions,
};

use crate::support::TestError;

/// Verifies builder defaults and convenience methods.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_builder_default_and_delay_helpers_work() {
    let retry = Retry::<TestError>::builder()
        .max_retries(2)
        .fixed_delay(Duration::from_millis(1))
        .jitter_factor(0.0)
        .build()
        .expect("retry should build");

    assert_eq!(retry.options().max_attempts(), 3);
    assert_eq!(
        retry.options().delay(),
        &RetryDelay::fixed(Duration::from_millis(1))
    );
    assert_eq!(retry.options().jitter(), RetryJitter::factor(0.0));
    assert_eq!(retry.options().attempt_timeout(), None);
    assert!(format!("{retry:?}").contains("Retry"));
}

/// Verifies builder replacement options and delay convenience variants.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when builder helpers set wrong options.
#[test]
fn test_builder_options_random_exponential_and_default_work() {
    let options = RetryOptions::new(
        2,
        Some(Duration::from_millis(42)),
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("retry options should be valid");
    let retry = Retry::<TestError>::from_options(options.clone()).expect("retry should build");
    assert_eq!(retry.options(), &options);

    let random = Retry::<TestError>::builder()
        .random_delay(Duration::from_millis(3), Duration::from_millis(5))
        .build()
        .expect("retry should build");
    assert_eq!(
        random.options().delay(),
        &RetryDelay::random(Duration::from_millis(3), Duration::from_millis(5))
    );

    let exponential = Retry::<TestError>::builder()
        .exponential_backoff(Duration::from_millis(10), Duration::from_millis(80))
        .build()
        .expect("retry should build");
    assert_eq!(
        exponential.options().delay(),
        &RetryDelay::exponential(Duration::from_millis(10), Duration::from_millis(80), 2.0)
    );

    let custom_exponential = Retry::<TestError>::builder()
        .exponential_backoff_with_multiplier(
            Duration::from_millis(10),
            Duration::from_millis(80),
            3.0,
        )
        .build()
        .expect("retry should build");
    assert_eq!(
        custom_exponential.options().delay(),
        &RetryDelay::exponential(Duration::from_millis(10), Duration::from_millis(80), 3.0)
    );

    let timeout = Retry::<TestError>::builder()
        .attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
        .attempt_timeout(Some(Duration::from_millis(7)))
        .build()
        .expect("retry with timeout should build");
    assert_eq!(
        timeout.options().attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(7)))
    );

    let default_builder: qubit_retry::RetryBuilder<TestError> = Default::default();
    assert_eq!(
        default_builder
            .build()
            .expect("default retry should build")
            .options()
            .max_attempts(),
        DEFAULT_RETRY_MAX_ATTEMPTS
    );
}

/// Verifies builder validation rejects invalid attempt counts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_build_validates_max_attempts_and_options() {
    let error = Retry::<TestError>::builder()
        .max_attempts(0)
        .build()
        .expect_err("zero max attempts should be rejected");
    assert!(error.to_string().contains("max_attempts"));

    let invalid = RetryOptions::new(
        3,
        None,
        RetryDelay::fixed(Duration::ZERO),
        RetryJitter::none(),
    );
    assert!(invalid.is_err());
}

/// Verifies timeout convenience methods configure timeout policies.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_timeout_convenience_methods_work() {
    let retry_abort = Retry::<TestError>::builder()
        .attempt_timeout(Some(Duration::from_millis(1)))
        .abort_on_timeout()
        .build()
        .expect("retry should build");
    let retry_continue = Retry::<TestError>::builder()
        .attempt_timeout(Some(Duration::from_millis(1)))
        .retry_on_timeout()
        .build()
        .expect("retry should build");

    assert_eq!(
        retry_abort.options().attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(1)))
    );
    assert_eq!(
        retry_continue.options().attempt_timeout(),
        Some(AttemptTimeoutOption::retry(Duration::from_millis(1)))
    );

    let abort_decision = retry_abort
        .run(|| -> Result<(), TestError> { Err(TestError("error")) })
        .expect_err("non-timeout should use defaults");
    assert_eq!(abort_decision.attempts(), DEFAULT_RETRY_MAX_ATTEMPTS);

    let continue_decision = retry_continue
        .run(|| -> Result<(), TestError> { Err(TestError("error")) })
        .expect_err("non-timeout should use defaults");
    assert_eq!(continue_decision.attempts(), DEFAULT_RETRY_MAX_ATTEMPTS);
}

/// Verifies custom failure listeners can be registered with rs-function traits.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_on_failure_accepts_function_trait() {
    struct AbortFatal;

    impl
        qubit_function::BiFunction<
            AttemptFailure<TestError>,
            qubit_retry::RetryContext,
            AttemptFailureDecision,
        > for AbortFatal
    {
        /// Applies the test decider.
        ///
        /// # Parameters
        /// - `failure`: Failure being handled.
        /// - `_context`: Retry context.
        ///
        /// # Returns
        /// Abort for fatal errors, otherwise use the default policy.
        fn apply(
            &self,
            failure: &AttemptFailure<TestError>,
            _context: &qubit_retry::RetryContext,
        ) -> AttemptFailureDecision {
            match failure {
                AttemptFailure::Error(TestError("fatal")) => AttemptFailureDecision::Abort,
                _ => AttemptFailureDecision::UseDefault,
            }
        }
    }

    let retry = Retry::<TestError>::builder()
        .on_failure(AbortFatal)
        .build()
        .expect("retry should build");
    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("fatal")) })
        .expect_err("fatal error should abort");
    assert_eq!(error.attempts(), 1);
}

/// Verifies `retry_if_error` can both retry and abort application errors.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the predicate decision is ignored.
#[test]
fn test_retry_if_error_retries_true_and_aborts_false() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .retry_if_error(|error: &TestError, context: &qubit_retry::RetryContext| {
            error.0 == "retry" && context.attempt() == 1
        })
        .build()
        .expect("retry should build");
    let mut attempts = 0;

    let error = retry
        .run(|| -> Result<(), TestError> {
            attempts += 1;
            if attempts == 1 {
                Err(TestError("retry"))
            } else {
                Err(TestError("stop"))
            }
        })
        .expect_err("second error should abort");

    assert_eq!(attempts, 2);
    assert_eq!(error.attempts(), 2);
    assert_eq!(error.last_error(), Some(&TestError("stop")));
}

/// Verifies `retry_if_error` keeps timeout failures on the default policy path.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout failures reach the error predicate.
#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_retry_if_error_uses_default_for_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .retry_if_error(|_error: &TestError, _context: &qubit_retry::RetryContext| false)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<(), TestError>(())
        })
        .await
        .expect_err("timeout should use default attempt limit");

    assert_eq!(error.attempts(), 1);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies timeout retry convenience handles actual timeout failures.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout retry decisions are wrong.
#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_retry_on_timeout_retries_timeout_failures() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .retry_on_timeout()
        .fixed_delay(Duration::from_millis(1))
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<(), TestError>(())
        })
        .await
        .expect_err("timed-out attempts should exhaust retry limit");

    assert_eq!(error.attempts(), 2);
    assert!(matches!(
        error.last_failure(),
        Some(AttemptFailure::Timeout)
    ));
}

/// Verifies listener panic isolation substitutes default listener outcomes.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when isolated listener panics escape.
#[test]
fn test_isolate_listener_panics_suppresses_listener_panics() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .isolate_listener_panics()
        .before_attempt(|_context: &qubit_retry::RetryContext| panic!("before panic"))
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &qubit_retry::RetryContext| {
                panic!("failure panic")
            },
        )
        .on_retry(
            |_failure: &AttemptFailure<TestError>, _context: &qubit_retry::RetryContext| {
                panic!("retry panic")
            },
        )
        .on_error(
            |_error: &qubit_retry::RetryError<TestError>, _context: &qubit_retry::RetryContext| {
                panic!("error panic")
            },
        )
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("isolated")) })
        .expect_err("operation error should still be returned");

    assert_eq!(error.attempts(), 2);
    assert_eq!(error.last_error(), Some(&TestError("isolated")));
}

/// Verifies `options()` carries timeout policy into later timeout duration updates.
#[test]
fn test_options_sets_pending_attempt_timeout_policy() {
    let options = RetryOptions::new_with_attempt_timeout(
        2,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(9))),
    )
    .expect("retry options should be valid");

    let retry = Retry::<TestError>::builder()
        .options(options)
        .attempt_timeout(Some(Duration::from_millis(7)))
        .build()
        .expect("retry should build");

    assert_eq!(
        retry.options().attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(7)))
    );
}

/// Verifies explicit timeout option also updates pending timeout policy.
#[test]
fn test_attempt_timeout_option_updates_pending_policy_for_later_duration() {
    let retry = Retry::<TestError>::builder()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(Duration::from_millis(3))))
        .attempt_timeout(Some(Duration::from_millis(5)))
        .build()
        .expect("retry should build");

    assert_eq!(
        retry.options().attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(5)))
    );
}

/// Verifies `build()` surfaces validation errors from merged options.
#[test]
fn test_build_propagates_option_validation_errors() {
    let error = Retry::<TestError>::builder()
        .jitter_factor(1.5)
        .build()
        .expect_err("invalid jitter factor should be rejected");

    assert!(error.to_string().contains("jitter"));
}
