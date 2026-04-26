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
    AttemptTimeoutOption, AttemptTimeoutPolicy, RetryConfigValues, RetryDelay, RetryJitter,
    RetryOptions,
};

fn sample_retry_config_values_none_delay() -> RetryConfigValues {
    RetryConfigValues {
        max_attempts: None,
        max_elapsed_millis: None,
        max_elapsed_unlimited: None,
        attempt_timeout_millis: None,
        attempt_timeout_policy: None,
        delay: Some("none".to_string()),
        delay_strategy: None,
        fixed_delay_millis: None,
        random_min_delay_millis: None,
        random_max_delay_millis: None,
        exponential_initial_delay_millis: None,
        exponential_max_delay_millis: None,
        exponential_multiplier: None,
        jitter_factor: None,
    }
}

/// Verifies missing `max_elapsed_millis` inherits `default.max_elapsed` in [`RetryConfigValues::to_options`].
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when merge behavior is incorrect.
#[test]
fn test_to_options_missing_max_elapsed_millis_uses_default_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let options = sample_retry_config_values_none_delay()
        .to_options(&default)
        .expect("valid merged options");
    assert_eq!(options.max_elapsed(), Some(Duration::from_secs(42)));
}

/// Verifies `max_elapsed_millis` of zero means zero elapsed-time budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when zero-budget semantics are incorrect.
#[test]
fn test_to_options_zero_max_elapsed_millis_means_zero_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.max_elapsed_millis = Some(0);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.max_elapsed(), Some(Duration::ZERO));
}

/// Verifies `max_elapsed_unlimited=true` forces unlimited budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when unlimited override is ignored.
#[test]
fn test_to_options_max_elapsed_unlimited_overrides_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.max_elapsed_millis = Some(0);
    values.max_elapsed_unlimited = Some(true);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.max_elapsed(), None);
}

/// Verifies timeout values merge with default timeout policy.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when timeout merge behavior is incorrect.
#[test]
fn test_to_options_attempt_timeout_uses_default_policy() {
    let default = RetryOptions::new_with_attempt_timeout(
        2,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
        Some(AttemptTimeoutOption::abort(Duration::from_secs(1))),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.attempt_timeout_millis = Some(50);

    let options = values.to_options(&default).expect("valid merged options");

    assert_eq!(
        options.attempt_timeout(),
        Some(AttemptTimeoutOption::abort(Duration::from_millis(50)))
    );
}

/// Verifies timeout policy can override a default timeout duration.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when policy override behavior is incorrect.
#[test]
fn test_to_options_attempt_timeout_policy_overrides_default_timeout() {
    let default = RetryOptions::new_with_attempt_timeout(
        2,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
        Some(AttemptTimeoutOption::abort(Duration::from_secs(1))),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.attempt_timeout_policy = Some("retry".to_string());

    let options = values.to_options(&default).expect("valid merged options");

    assert_eq!(
        options.attempt_timeout(),
        Some(AttemptTimeoutOption::new(
            Duration::from_secs(1),
            AttemptTimeoutPolicy::Retry,
        ))
    );
}
