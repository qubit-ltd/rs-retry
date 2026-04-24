/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, Retry, RetryDelay, RetryJitter, RetryOptions,
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
    assert!(format!("{retry:?}").contains("Retry"));
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

/// Verifies timeout convenience listeners map to failure decisions.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_timeout_convenience_methods_work() {
    let retry_abort = Retry::<TestError>::builder()
        .abort_on_timeout()
        .build()
        .expect("retry should build");
    let retry_continue = Retry::<TestError>::builder()
        .retry_on_timeout()
        .build()
        .expect("retry should build");

    let abort_decision = retry_abort
        .run(|| -> Result<(), TestError> { Err(TestError("error")) })
        .expect_err("non-timeout should use defaults");
    assert_eq!(abort_decision.attempts(), 3);

    let continue_decision = retry_continue
        .run(|| -> Result<(), TestError> { Err(TestError("error")) })
        .expect_err("non-timeout should use defaults");
    assert_eq!(continue_decision.attempts(), 3);
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
