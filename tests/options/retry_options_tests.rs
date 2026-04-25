/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_config::Config;
use qubit_retry::constants::{
    KEY_DELAY, KEY_DELAY_STRATEGY, KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS,
    KEY_EXPONENTIAL_MAX_DELAY_MILLIS, KEY_EXPONENTIAL_MULTIPLIER, KEY_FIXED_DELAY_MILLIS,
    KEY_JITTER_FACTOR, KEY_MAX_ATTEMPTS, KEY_MAX_ELAPSED_MILLIS, KEY_MAX_ELAPSED_UNLIMITED,
    KEY_RANDOM_MAX_DELAY_MILLIS, KEY_RANDOM_MIN_DELAY_MILLIS,
};
use qubit_retry::{RetryDelay, RetryJitter, RetryOptions};

/// Verifies default options and direct construction.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when validation or construction behavior
/// is incorrect.
#[test]
fn test_validate_default_and_new() {
    let options = RetryOptions::default();
    assert_eq!(options.max_attempts(), 3);
    assert_eq!(options.max_elapsed(), None);
    assert!(matches!(options.jitter(), RetryJitter::None));

    let options = RetryOptions::new(2, None, RetryDelay::none(), RetryJitter::none())
        .expect("valid retry options should be created");
    assert_eq!(options.max_attempts(), 2);

    let zero = RetryOptions::new(0, None, RetryDelay::none(), RetryJitter::none())
        .expect_err("zero attempts should be rejected");
    assert_eq!(zero.path(), KEY_MAX_ATTEMPTS);

    let invalid_jitter =
        RetryOptions::new(2, None, RetryDelay::none(), RetryJitter::factor(f64::NAN))
            .expect_err("invalid jitter should be rejected");
    assert_eq!(invalid_jitter.path(), KEY_JITTER_FACTOR);
}

/// Verifies prefixed configuration values are read into fixed-delay options.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when prefixed config values are parsed
/// incorrectly.
#[test]
fn test_from_config_reads_fixed_delay_from_prefixed_config() {
    let mut config = Config::new();
    config
        .set("retry.max_attempts", 4u32)
        .expect("test config value should be set");
    config
        .set("retry.max_elapsed_millis", 250u64)
        .expect("test config value should be set");
    config
        .set("retry.delay", "fixed")
        .expect("test config value should be set");
    config
        .set("retry.fixed_delay_millis", 15u64)
        .expect("test config value should be set");
    config
        .set("retry.jitter_factor", 0.25)
        .expect("test config value should be set");

    let options = RetryOptions::from_config(&config.prefix_view("retry"))
        .expect("prefixed retry config should be parsed");

    assert_eq!(options.max_attempts(), 4);
    assert_eq!(options.max_elapsed(), Some(Duration::from_millis(250)));
    assert_eq!(
        options.delay(),
        &RetryDelay::fixed(Duration::from_millis(15))
    );
    assert_eq!(options.jitter(), RetryJitter::factor(0.25));
}

/// Verifies non-fixed delay config forms and config read errors.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when delay forms or config errors are
/// handled incorrectly.
#[test]
fn test_from_config_reads_other_delay_forms_and_reports_config_errors() {
    let mut random_config = Config::new();
    random_config
        .set("delay", "random")
        .expect("test config value should be set");
    random_config
        .set("random_min_delay_millis", 3u64)
        .expect("test config value should be set");
    random_config
        .set("random_max_delay_millis", 9u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&random_config)
            .expect("random delay config should be parsed")
            .delay(),
        &RetryDelay::random(Duration::from_millis(3), Duration::from_millis(9))
    );

    let mut exponential_config = Config::new();
    exponential_config
        .set("delay_strategy", "exponential_backoff")
        .expect("test config value should be set");
    exponential_config
        .set("exponential_initial_delay_millis", 10u64)
        .expect("test config value should be set");
    exponential_config
        .set("exponential_max_delay_millis", 80u64)
        .expect("test config value should be set");
    exponential_config
        .set("exponential_multiplier", 3.0)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&exponential_config)
            .expect("exponential delay config should be parsed")
            .delay(),
        &RetryDelay::exponential(Duration::from_millis(10), Duration::from_millis(80), 3.0)
    );

    let mut implicit_config = Config::new();
    implicit_config
        .set("fixed_delay_millis", 6u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&implicit_config)
            .expect("implicit fixed delay config should be parsed")
            .delay(),
        &RetryDelay::fixed(Duration::from_millis(6))
    );

    let mut disabled_elapsed = Config::new();
    disabled_elapsed
        .set("max_elapsed_millis", 0u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&disabled_elapsed)
            .expect("zero max elapsed should be allowed")
            .max_elapsed(),
        Some(Duration::ZERO)
    );

    let mut unlimited_elapsed = Config::new();
    unlimited_elapsed
        .set("max_elapsed_millis", 0u64)
        .expect("test config value should be set");
    unlimited_elapsed
        .set("max_elapsed_unlimited", true)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&unlimited_elapsed)
            .expect("explicit unlimited max elapsed should be allowed")
            .max_elapsed(),
        None
    );

    let mut zero_jitter = Config::new();
    zero_jitter
        .set("jitter_factor", 0.0)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&zero_jitter)
            .expect("zero jitter should be parsed")
            .jitter(),
        RetryJitter::None
    );

    let mut invalid_strategy = Config::new();
    invalid_strategy
        .set("delay", "linear")
        .expect("test config value should be set");
    let error = RetryOptions::from_config(&invalid_strategy)
        .expect_err("unsupported delay strategy should fail");
    assert_eq!(error.path(), KEY_DELAY);
    assert!(error.message().contains("unsupported"));

    let mut bad_type = Config::new();
    bad_type
        .set("max_attempts", "not-a-number")
        .expect("test config value should be set");
    let error =
        RetryOptions::from_config(&bad_type).expect_err("wrong max_attempts type should fail");
    assert_eq!(error.path(), KEY_MAX_ATTEMPTS);

    let mut unlimited_bad_type = Config::new();
    unlimited_bad_type
        .set("max_elapsed_unlimited", "bad")
        .expect("test config value should be set");
    let error = RetryOptions::from_config(&unlimited_bad_type)
        .expect_err("wrong max_elapsed_unlimited type should fail");
    assert_eq!(error.path(), KEY_MAX_ELAPSED_UNLIMITED);
}

/// Verifies explicit and implicit delay defaults from configuration.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when default delay values are applied
/// incorrectly.
#[test]
fn test_from_config_reads_explicit_and_implicit_delay_defaults() {
    let mut fixed_default = Config::new();
    fixed_default
        .set("delay", "fixed")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&fixed_default)
            .expect("fixed delay defaults should be parsed")
            .delay(),
        &RetryDelay::fixed(Duration::from_millis(1000))
    );

    let mut random_default = Config::new();
    random_default
        .set("delay", "random")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&random_default)
            .expect("random delay defaults should be parsed")
            .delay(),
        &RetryDelay::random(Duration::from_millis(1000), Duration::from_millis(10000))
    );

    let mut exponential_default = Config::new();
    exponential_default
        .set("delay", "exponential")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&exponential_default)
            .expect("exponential delay defaults should be parsed")
            .delay(),
        &RetryDelay::exponential(
            Duration::from_millis(1000),
            Duration::from_millis(60000),
            2.0
        )
    );

    let mut implicit_random = Config::new();
    implicit_random
        .set("random_max_delay_millis", 12000u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&implicit_random)
            .expect("implicit random delay should be parsed")
            .delay(),
        &RetryDelay::random(Duration::from_millis(1000), Duration::from_millis(12000))
    );

    let mut implicit_exponential = Config::new();
    implicit_exponential
        .set("exponential_multiplier", 4.0)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&implicit_exponential)
            .expect("implicit exponential delay should be parsed")
            .delay(),
        &RetryDelay::exponential(
            Duration::from_millis(1000),
            Duration::from_millis(60000),
            4.0
        )
    );
}

/// Verifies delay parameter type errors report the exact config key.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when type errors are accepted or reported
/// with the wrong path.
#[test]
fn test_from_config_reports_delay_parameter_type_errors() {
    let mut elapsed_bad = Config::new();
    elapsed_bad
        .set("max_elapsed_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&elapsed_bad)
            .expect_err("invalid max elapsed type should fail")
            .path(),
        KEY_MAX_ELAPSED_MILLIS
    );

    let mut delay_bad = Config::new();
    delay_bad
        .set("delay", 123u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&delay_bad)
            .expect_err("invalid delay type should fail")
            .path(),
        KEY_DELAY
    );

    let mut delay_strategy_bad = Config::new();
    delay_strategy_bad
        .set("delay_strategy", 123u64)
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&delay_strategy_bad)
            .expect_err("invalid delay strategy type should fail")
            .path(),
        KEY_DELAY_STRATEGY
    );

    let mut fixed_bad = Config::new();
    fixed_bad
        .set("delay", "fixed")
        .expect("test config value should be set");
    fixed_bad
        .set("fixed_delay_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&fixed_bad)
            .expect_err("invalid fixed delay type should fail")
            .path(),
        KEY_FIXED_DELAY_MILLIS
    );

    let mut random_min_bad = Config::new();
    random_min_bad
        .set("delay", "random")
        .expect("test config value should be set");
    random_min_bad
        .set("random_min_delay_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&random_min_bad)
            .expect_err("invalid random min delay type should fail")
            .path(),
        KEY_RANDOM_MIN_DELAY_MILLIS
    );

    let mut random_max_bad = Config::new();
    random_max_bad
        .set("delay", "random")
        .expect("test config value should be set");
    random_max_bad
        .set("random_max_delay_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&random_max_bad)
            .expect_err("invalid random max delay type should fail")
            .path(),
        KEY_RANDOM_MAX_DELAY_MILLIS
    );

    let mut exponential_initial_bad = Config::new();
    exponential_initial_bad
        .set("delay", "exponential")
        .expect("test config value should be set");
    exponential_initial_bad
        .set("exponential_initial_delay_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&exponential_initial_bad)
            .expect_err("invalid exponential initial delay type should fail")
            .path(),
        KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS
    );

    let mut exponential_max_bad = Config::new();
    exponential_max_bad
        .set("delay", "exponential")
        .expect("test config value should be set");
    exponential_max_bad
        .set("exponential_max_delay_millis", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&exponential_max_bad)
            .expect_err("invalid exponential max delay type should fail")
            .path(),
        KEY_EXPONENTIAL_MAX_DELAY_MILLIS
    );

    let mut exponential_multiplier_bad = Config::new();
    exponential_multiplier_bad
        .set("delay", "exponential")
        .expect("test config value should be set");
    exponential_multiplier_bad
        .set("exponential_multiplier", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&exponential_multiplier_bad)
            .expect_err("invalid exponential multiplier type should fail")
            .path(),
        KEY_EXPONENTIAL_MULTIPLIER
    );

    let mut jitter_bad = Config::new();
    jitter_bad
        .set("jitter_factor", "bad")
        .expect("test config value should be set");
    assert_eq!(
        RetryOptions::from_config(&jitter_bad)
            .expect_err("invalid jitter factor type should fail")
            .path(),
        KEY_JITTER_FACTOR
    );
}

/// Verifies retry delay calculation helpers on [`RetryOptions`].
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when helper methods compute wrong delays.
#[test]
fn test_retry_options_delay_calculation_helpers() {
    let options = RetryOptions::new(
        4,
        None,
        RetryDelay::exponential(Duration::from_millis(10), Duration::from_millis(80), 2.0),
        RetryJitter::none(),
    )
    .expect("retry options should be valid");

    assert_eq!(options.base_delay_for_attempt(1), Duration::from_millis(10));
    assert_eq!(options.base_delay_for_attempt(4), Duration::from_millis(80));
    assert_eq!(options.delay_for_attempt(2), Duration::from_millis(20));
    assert_eq!(
        options.next_base_delay_from_current(Duration::from_millis(40)),
        Duration::from_millis(80)
    );
    assert_eq!(
        options.next_base_delay_from_current(Duration::from_millis(200)),
        Duration::from_millis(80)
    );
    assert_eq!(
        options.jittered_delay(Duration::from_millis(15)),
        Duration::from_millis(15)
    );
    assert_eq!(
        options.next_delay_from_current(Duration::from_millis(10)),
        Duration::from_millis(20)
    );

    let fixed = RetryOptions::new(
        3,
        None,
        RetryDelay::fixed(Duration::from_millis(7)),
        RetryJitter::none(),
    )
    .expect("fixed retry options should be valid");
    assert_eq!(
        fixed.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::from_millis(7)
    );

    let none = RetryOptions::new(3, None, RetryDelay::none(), RetryJitter::none())
        .expect("none retry options should be valid");
    assert_eq!(
        none.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::ZERO
    );

    let random = RetryOptions::new(
        3,
        None,
        RetryDelay::random(Duration::from_millis(4), Duration::from_millis(4)),
        RetryJitter::none(),
    )
    .expect("random retry options should be valid");
    assert_eq!(
        random.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::from_millis(4)
    );
}
