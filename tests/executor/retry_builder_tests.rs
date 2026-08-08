// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::AttemptTimeoutOption;
use qubit_retry::AttemptTimeoutPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryBuilder;
use qubit_retry::RetryContext;
use qubit_retry::RetryDelay;
use qubit_retry::RetryError;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryJitter;
use qubit_retry::RetryOptions;
use qubit_retry::constants::DEFAULT_RETRY_MAX_ATTEMPTS;

use crate::support::AbortFatal;
use crate::support::FixedRetryRandomSource;
use crate::support::TestError;

/// Verifies a builder-injected random source controls runner delay selection.
#[test]
fn test_random_source_controls_retry_delay_selection() {
    let observed_delay = Arc::new(Mutex::new(None));
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .random_delay(Duration::from_nanos(5), Duration::from_nanos(9))
        .random_source(Arc::new(FixedRetryRandomSource::new(7, 0.0)))
        .on_retry({
            let observed_delay = Arc::clone(&observed_delay);
            move |_failure: &AttemptFailure<TestError>,
                  context: &RetryContext| {
                *observed_delay
                    .lock()
                    .expect("observed delay should be lockable") =
                    context.next_delay();
            }
        })
        .build()
        .expect("retry should build");
    let mut attempts = 0;

    let value = retry
        .run(|| {
            attempts += 1;
            if attempts == 1 {
                Err(TestError("retry"))
            } else {
                Ok("done")
            }
        })
        .expect("second attempt should succeed");

    assert_eq!(value.into_value(), "done");
    assert_eq!(attempts, 2);
    assert_eq!(
        *observed_delay
            .lock()
            .expect("observed delay should be lockable"),
        Some(Duration::from_nanos(7))
    );
}

/// Verifies builder defaults and convenience methods.
#[test]
fn test_builder_default_and_delay_helpers_work() {
    let retry = Retry::<TestError>::builder()
        .max_retries(2)
        .max_total_elapsed(Some(Duration::from_secs(5)))
        .fixed_delay(Duration::from_millis(1))
        .jitter_factor(0.0)
        .worker_cancel_grace(Duration::from_millis(25))
        .build()
        .expect("retry should build");

    assert_eq!(retry.options().max_attempts(), 3);
    assert_eq!(
        retry.options().max_total_elapsed(),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        retry.options().delay(),
        &RetryDelay::fixed(Duration::from_millis(1))
    );
    assert_eq!(retry.options().jitter(), RetryJitter::factor(0.0));
    assert_eq!(retry.options().attempt_timeout(), None);
    assert_eq!(
        retry.options().worker_cancel_grace(),
        Duration::from_millis(25)
    );
    assert!(format!("{retry:?}").contains("Retry"));
}

/// Verifies builder replacement options and delay convenience variants.
#[test]
fn test_builder_options_random_exponential_and_default_work() {
    let options = RetryOptions::new(
        2,
        Some(Duration::from_millis(42)),
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("retry options should be valid");
    let retry = Retry::<TestError>::from_options(options.clone())
        .expect("retry should build");
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
        .exponential_backoff(
            Duration::from_millis(10),
            Duration::from_millis(80),
        )
        .build()
        .expect("retry should build");
    assert_eq!(
        exponential.options().delay(),
        &RetryDelay::exponential(
            Duration::from_millis(10),
            Duration::from_millis(80),
            2.0
        )
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
        &RetryDelay::exponential(
            Duration::from_millis(10),
            Duration::from_millis(80),
            3.0
        )
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

    let default_builder: RetryBuilder<TestError> = Default::default();
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
        None,
        RetryDelay::fixed(Duration::ZERO),
        RetryJitter::none(),
    );
    assert!(invalid.is_err());
}

/// Verifies timeout convenience methods configure timeout policies.
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
        .expect_err("run with attempt timeout must be unsupported");
    assert_eq!(
        abort_decision.reason(),
        RetryErrorReason::UnsupportedOperation
    );
    assert_eq!(abort_decision.attempts(), 0);

    let continue_decision = retry_continue
        .run(|| -> Result<(), TestError> { Err(TestError("error")) })
        .expect_err("run with attempt timeout must be unsupported");
    assert_eq!(
        continue_decision.reason(),
        RetryErrorReason::UnsupportedOperation
    );
    assert_eq!(continue_decision.attempts(), 0);
}

/// Verifies custom failure listeners can be registered with rs-function traits.
#[test]
fn test_on_failure_accepts_function_trait() {
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
#[test]
fn test_retry_if_error_retries_true_and_aborts_false() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .no_delay()
        .retry_if_error(|error: &TestError, context: &RetryContext| {
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
#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn test_retry_if_error_uses_default_for_timeout() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(1)))
        .retry_if_error(|_error: &TestError, _context: &RetryContext| false)
        .no_delay()
        .build()
        .expect("retry should build");

    let error = retry
        .run_async(|| async {
            // Tokio time is paused, so this is deterministic virtual time.
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
            // Tokio time is paused, so this is deterministic virtual time.
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
#[test]
fn test_isolate_listener_panics_suppresses_listener_panics() {
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .no_delay()
        .isolate_listener_panics()
        .before_attempt(|_context: &RetryContext| panic!("before panic"))
        .on_failure(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                panic!("failure panic")
            },
        )
        .on_retry(
            |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
                panic!("retry panic")
            },
        )
        .on_error(|_error: &RetryError<TestError>, _context: &RetryContext| {
            panic!("error panic")
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("isolated")) })
        .expect_err("operation error should still be returned");

    assert_eq!(error.attempts(), 2);
    assert_eq!(error.last_error(), Some(&TestError("isolated")));
}

/// Verifies `options()` carries timeout policy into later timeout duration
/// updates.
#[test]
fn test_options_sets_pending_attempt_timeout_policy() {
    let options = RetryOptions::new_with_attempt_timeout(
        2,
        None,
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
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(3),
        )))
        .attempt_timeout(Some(Duration::from_millis(5)))
        .build()
        .expect("retry should build");

    assert_eq!(
        retry.options().attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(5)))
    );
}

/// Verifies clearing the timeout option resets the pending timeout policy.
#[test]
fn test_attempt_timeout_option_none_resets_pending_policy_for_later_duration() {
    let retry = Retry::<TestError>::builder()
        .attempt_timeout_option(Some(AttemptTimeoutOption::abort(
            Duration::from_millis(3),
        )))
        .attempt_timeout_option(None)
        .attempt_timeout(Some(Duration::from_millis(5)))
        .build()
        .expect("retry should build");

    assert_eq!(
        retry.options().attempt_timeout(),
        Some(AttemptTimeoutOption::retry(Duration::from_millis(5)))
    );
}

/// Verifies clearing timeout duration resets the pending timeout policy.
#[test]
fn test_attempt_timeout_none_resets_pending_policy_for_later_duration() {
    let retry = Retry::<TestError>::builder()
        .attempt_timeout_policy(AttemptTimeoutPolicy::Abort)
        .attempt_timeout(None)
        .attempt_timeout(Some(Duration::from_millis(5)))
        .build()
        .expect("retry should build");

    assert_eq!(
        retry.options().attempt_timeout(),
        Some(AttemptTimeoutOption::retry(Duration::from_millis(5)))
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
