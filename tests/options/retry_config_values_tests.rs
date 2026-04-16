/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::{RetryConfigValues, RetryDelay, RetryJitter, RetryOptions};

fn sample_retry_config_values_none_delay() -> RetryConfigValues {
    RetryConfigValues {
        max_attempts: None,
        max_elapsed_millis: None,
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
    assert_eq!(options.max_elapsed, Some(Duration::from_secs(42)));
}

/// Verifies `max_elapsed_millis` of zero overrides a non-empty default budget to unlimited.
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
fn test_to_options_zero_max_elapsed_millis_overrides_default_to_unlimited() {
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
    assert_eq!(options.max_elapsed, None);
}
