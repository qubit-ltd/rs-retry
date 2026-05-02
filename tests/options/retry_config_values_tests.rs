/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::constants::{
    KEY_ATTEMPT_TIMEOUT_POLICY, KEY_DELAY, KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS,
    KEY_EXPONENTIAL_MAX_DELAY_MILLIS, KEY_EXPONENTIAL_MULTIPLIER, KEY_FIXED_DELAY_MILLIS,
    KEY_RANDOM_MAX_DELAY_MILLIS, KEY_RANDOM_MIN_DELAY_MILLIS,
};
use qubit_retry::{
    AttemptTimeoutOption, AttemptTimeoutPolicy, RetryConfigValues, RetryDelay, RetryJitter,
    RetryOptions,
};

fn sample_retry_config_values_none_delay() -> RetryConfigValues {
    RetryConfigValues {
        max_attempts: None,
        max_operation_elapsed_millis: None,
        max_operation_elapsed_unlimited: None,
        max_total_elapsed_millis: None,
        max_total_elapsed_unlimited: None,
        attempt_timeout_millis: None,
        attempt_timeout_policy: None,
        worker_cancel_grace_millis: None,
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

/// Verifies worker cancel grace inherits from defaults unless configured.
#[test]
fn test_to_options_worker_cancel_grace_uses_config_or_default() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let options = sample_retry_config_values_none_delay()
        .to_options(&default)
        .expect("valid merged options");
    assert_eq!(options.worker_cancel_grace(), default.worker_cancel_grace());

    let mut values = sample_retry_config_values_none_delay();
    values.worker_cancel_grace_millis = Some(25);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.worker_cancel_grace(), Duration::from_millis(25));
}

/// Verifies missing `max_operation_elapsed_millis` inherits `default.max_operation_elapsed` in [`RetryConfigValues::to_options`].
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
fn test_to_options_missing_max_operation_elapsed_millis_uses_default_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let options = sample_retry_config_values_none_delay()
        .to_options(&default)
        .expect("valid merged options");
    assert_eq!(
        options.max_operation_elapsed(),
        Some(Duration::from_secs(42))
    );
}

/// Verifies `max_operation_elapsed_millis` of zero means zero elapsed-time budget.
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
fn test_to_options_zero_max_operation_elapsed_millis_means_zero_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.max_operation_elapsed_millis = Some(0);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.max_operation_elapsed(), Some(Duration::ZERO));
}

/// Verifies `max_operation_elapsed_unlimited=true` forces unlimited budget.
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
fn test_to_options_max_operation_elapsed_unlimited_overrides_budget() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.max_operation_elapsed_millis = Some(0);
    values.max_operation_elapsed_unlimited = Some(true);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.max_operation_elapsed(), None);
}

/// Verifies total elapsed budget config merges independently from operation elapsed budget.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when total elapsed merge behavior is incorrect.
#[test]
fn test_to_options_max_total_elapsed_merges_independently() {
    let default = RetryOptions::new(
        2,
        Some(Duration::from_secs(42)),
        Some(Duration::from_secs(84)),
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid default");
    let options = sample_retry_config_values_none_delay()
        .to_options(&default)
        .expect("valid merged options");
    assert_eq!(
        options.max_operation_elapsed(),
        Some(Duration::from_secs(42))
    );
    assert_eq!(options.max_total_elapsed(), Some(Duration::from_secs(84)));

    let mut values = sample_retry_config_values_none_delay();
    values.max_total_elapsed_millis = Some(0);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(
        options.max_operation_elapsed(),
        Some(Duration::from_secs(42))
    );
    assert_eq!(options.max_total_elapsed(), Some(Duration::ZERO));

    let mut values = sample_retry_config_values_none_delay();
    values.max_total_elapsed_millis = Some(0);
    values.max_total_elapsed_unlimited = Some(true);
    let options = values.to_options(&default).expect("valid merged options");
    assert_eq!(options.max_total_elapsed(), None);
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

/// Verifies timeout policy without timeout millis errors when defaults disable timeout.
#[test]
fn test_to_options_attempt_timeout_policy_requires_timeout_without_default() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.attempt_timeout_policy = Some("abort".to_string());

    let error = values
        .to_options(&default)
        .expect_err("policy alone should be rejected when default has no timeout");

    assert!(error.to_string().contains("attempt_timeout_policy"));
}

/// Verifies explicit fixed delay strategy requires `fixed_delay_millis`.
#[test]
fn test_to_options_explicit_fixed_delay_requires_fixed_delay_millis() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("fixed".to_string());

    let error = values
        .to_options(&default)
        .expect_err("fixed delay requires delay value");

    assert_eq!(error.path(), KEY_FIXED_DELAY_MILLIS);
    assert!(error.message().contains("fixed_delay_millis"));
}

/// Verifies explicit random delay strategy requires both random bounds.
#[test]
fn test_to_options_explicit_random_delay_requires_min_and_max_delay() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("random".to_string());
    values.random_min_delay_millis = Some(5);

    let error = values
        .to_options(&default)
        .expect_err("random delay requires both bounds");
    assert_eq!(error.path(), KEY_RANDOM_MAX_DELAY_MILLIS);
    assert!(error.message().contains("random_max_delay_millis"));

    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("random".to_string());
    values.random_max_delay_millis = Some(8);

    let error = values
        .to_options(&default)
        .expect_err("random delay requires both bounds");
    assert_eq!(error.path(), KEY_RANDOM_MIN_DELAY_MILLIS);
    assert!(error.message().contains("random_min_delay_millis"));
}

/// Verifies explicit exponential delay strategy requires all explicit parameters.
#[test]
fn test_to_options_explicit_exponential_delay_requires_all_parameters() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("exponential".to_string());
    values.exponential_initial_delay_millis = Some(1_000);
    values.exponential_max_delay_millis = Some(5_000);

    let error = values
        .to_options(&default)
        .expect_err("exponential delay requires multiplier");
    assert_eq!(error.path(), KEY_EXPONENTIAL_MULTIPLIER);
    assert!(error.message().contains("exponential_multiplier"));

    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("exponential_backoff".to_string());
    values.exponential_initial_delay_millis = Some(1_000);
    values.exponential_multiplier = Some(2.0);

    let error = values
        .to_options(&default)
        .expect_err("exponential delay requires max delay");
    assert_eq!(error.path(), KEY_EXPONENTIAL_MAX_DELAY_MILLIS);
    assert!(error.message().contains("exponential_max_delay_millis"));

    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("exponential".to_string());
    values.exponential_max_delay_millis = Some(5_000);
    values.exponential_multiplier = Some(2.0);

    let error = values
        .to_options(&default)
        .expect_err("exponential delay requires initial delay");
    assert_eq!(error.path(), KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS);
    assert!(error.message().contains("exponential_initial_delay_millis"));
}

/// Verifies unsupported explicit delay strategy fails fast with a parse error.
#[test]
fn test_to_options_invalid_delay_strategy_is_rejected() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.delay = Some("unknown".to_string());

    let error = values
        .to_options(&default)
        .expect_err("unsupported delay strategy should be rejected");

    assert_eq!(error.path(), KEY_DELAY);
    assert!(error.message().contains("unsupported delay strategy"));
}

/// Verifies unsupported attempt-timeout policy fails fast with a parse error.
#[test]
fn test_to_options_invalid_attempt_timeout_policy_is_rejected() {
    let default = RetryOptions::new(2, None, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid default");
    let mut values = sample_retry_config_values_none_delay();
    values.attempt_timeout_policy = Some("invalid-policy".to_string());

    let error = values
        .to_options(&default)
        .expect_err("unsupported attempt timeout policy should be rejected");

    assert_eq!(error.path(), KEY_ATTEMPT_TIMEOUT_POLICY);
    assert!(error.message().contains("attempt timeout"));
}
