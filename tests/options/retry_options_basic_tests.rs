// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::constants::{
    DEFAULT_RETRY_MAX_ATTEMPTS,
    KEY_ATTEMPT_TIMEOUT_MILLIS,
    KEY_DELAY,
    KEY_JITTER_FACTOR,
    KEY_MAX_ATTEMPTS,
};
use qubit_retry::{
    AttemptTimeoutOption,
    RetryDelay,
    RetryJitter,
    RetryOptions,
};

use crate::support::FixedRetryRandomSource;

/// Verifies option delay helpers accept an explicit random source.
#[test]
fn test_retry_options_delay_helpers_with_random_source() {
    let source = FixedRetryRandomSource::new(6_000_000, 1_000_000.0);
    let options = RetryOptions::new(
        3,
        None,
        None,
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8)),
        RetryJitter::factor(0.2),
    )
    .expect("retry options should build");

    assert_eq!(
        options.base_delay_for_attempt_with_random_source(1, &source),
        Duration::from_millis(6)
    );
    assert_eq!(
        options.delay_for_attempt_with_random_source(1, &source),
        Duration::from_millis(7)
    );
    assert_eq!(
        options.next_base_delay_from_current_with_random_source(
            Duration::from_millis(99),
            &source,
        ),
        Duration::from_millis(6)
    );
    assert_eq!(
        options.jittered_delay_with_random_source(
            Duration::from_millis(10),
            &source,
        ),
        Duration::from_millis(11)
    );
    assert_eq!(
        options.next_delay_from_current_with_random_source(
            Duration::from_millis(99),
            &source,
        ),
        Duration::from_millis(7)
    );
}

/// Verifies default retry options expose all default fields.
#[test]
fn test_retry_options_default_accessors() {
    let options = RetryOptions::default();

    assert_eq!(options.max_attempts(), DEFAULT_RETRY_MAX_ATTEMPTS);
    assert_eq!(options.max_operation_elapsed(), None);
    assert_eq!(options.max_total_elapsed(), None);
    assert_eq!(options.attempt_timeout(), None);
    assert_eq!(options.worker_cancel_grace(), Duration::from_millis(100));
    assert_eq!(options.jitter(), RetryJitter::none());
    assert_eq!(
        options.delay(),
        &RetryDelay::exponential(
            Duration::from_secs(1),
            Duration::from_secs(60),
            2.0
        )
    );
}

/// Verifies direct retry option construction and validation errors.
#[test]
fn test_retry_options_constructors_validate_invalid_values() {
    let options = RetryOptions::new(
        3,
        Some(Duration::from_millis(500)),
        Some(Duration::from_secs(2)),
        RetryDelay::fixed(Duration::from_millis(10)),
        RetryJitter::factor(0.25),
    )
    .expect("valid retry options should be accepted");
    assert_eq!(options.max_attempts(), 3);
    assert_eq!(
        options.max_operation_elapsed(),
        Some(Duration::from_millis(500))
    );
    assert_eq!(options.max_total_elapsed(), Some(Duration::from_secs(2)));
    assert_eq!(
        options.delay(),
        &RetryDelay::fixed(Duration::from_millis(10))
    );
    assert_eq!(options.jitter(), RetryJitter::factor(0.25));

    let invalid_attempts = RetryOptions::new(
        0,
        None,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect_err("zero max attempts should be rejected");
    assert_eq!(invalid_attempts.path(), KEY_MAX_ATTEMPTS);

    let invalid_delay = RetryOptions::new(
        2,
        None,
        None,
        RetryDelay::fixed(Duration::ZERO),
        RetryJitter::none(),
    )
    .expect_err("invalid delay should be rejected");
    assert_eq!(invalid_delay.path(), KEY_DELAY);

    let invalid_jitter = RetryOptions::new(
        2,
        None,
        None,
        RetryDelay::none(),
        RetryJitter::factor(f64::NAN),
    )
    .expect_err("invalid jitter should be rejected");
    assert_eq!(invalid_jitter.path(), KEY_JITTER_FACTOR);

    let invalid_timeout = RetryOptions::new_with_attempt_timeout(
        2,
        None,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
        Some(AttemptTimeoutOption::retry(Duration::ZERO)),
    )
    .expect_err("zero attempt timeout should be rejected");
    assert_eq!(invalid_timeout.path(), KEY_ATTEMPT_TIMEOUT_MILLIS);
}

/// Verifies retry delay helpers on direct retry options.
#[test]
fn test_retry_options_delay_helpers() {
    let exponential = RetryOptions::new(
        4,
        None,
        None,
        RetryDelay::exponential(
            Duration::from_millis(10),
            Duration::from_millis(80),
            2.0,
        ),
        RetryJitter::none(),
    )
    .expect("valid exponential retry options should be accepted");
    assert_eq!(
        exponential.base_delay_for_attempt(1),
        Duration::from_millis(10)
    );
    assert_eq!(
        exponential.base_delay_for_attempt(4),
        Duration::from_millis(80)
    );
    assert_eq!(exponential.delay_for_attempt(2), Duration::from_millis(20));
    assert_eq!(
        exponential.next_base_delay_from_current(Duration::ZERO),
        Duration::from_millis(10)
    );
    assert_eq!(
        exponential.next_base_delay_from_current(Duration::from_millis(40)),
        Duration::from_millis(80)
    );
    assert_eq!(
        exponential.next_base_delay_from_current(Duration::from_millis(200)),
        Duration::from_millis(80)
    );
    assert_eq!(
        exponential.jittered_delay(Duration::from_millis(15)),
        Duration::from_millis(15)
    );
    assert_eq!(
        exponential.next_delay_from_current(Duration::from_millis(10)),
        Duration::from_millis(20)
    );

    let fixed = RetryOptions::new(
        3,
        None,
        None,
        RetryDelay::fixed(Duration::from_millis(7)),
        RetryJitter::none(),
    )
    .expect("valid fixed retry options should be accepted");
    assert_eq!(
        fixed.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::from_millis(7)
    );

    let none = RetryOptions::new(
        3,
        None,
        None,
        RetryDelay::none(),
        RetryJitter::none(),
    )
    .expect("valid no-delay options should be accepted");
    assert_eq!(
        none.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::ZERO
    );

    let random = RetryOptions::new(
        3,
        None,
        None,
        RetryDelay::random(Duration::from_millis(4), Duration::from_millis(4)),
        RetryJitter::none(),
    )
    .expect("valid random retry options should be accepted");
    assert_eq!(
        random.next_base_delay_from_current(Duration::from_millis(99)),
        Duration::from_millis(4)
    );
}

/// Verifies exponential delay advancement saturates instead of overflowing.
#[test]
fn test_next_base_delay_from_current_saturates_at_duration_max() {
    let options = RetryOptions::new(
        3,
        None,
        None,
        RetryDelay::exponential(Duration::from_secs(1), Duration::MAX, 2.0),
        RetryJitter::none(),
    )
    .expect("retry options should build");

    assert_eq!(
        Duration::MAX,
        options.next_base_delay_from_current(Duration::MAX)
    );
}
