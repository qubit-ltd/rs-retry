/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use std::str::FromStr;
use std::time::Duration;

use qubit_retry::RetryDelay;
use qubit_retry::constants::DEFAULT_RETRY_DELAY;

/// Verifies every delay variant calculates the expected base delay.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when a delay calculation is incorrect.
#[test]
fn test_base_delay_none_fixed_random_and_exponential_values() {
    assert_eq!(RetryDelay::none().base_delay(1), Duration::ZERO);
    assert_eq!(
        RetryDelay::fixed(Duration::from_millis(12)).base_delay(9),
        Duration::from_millis(12)
    );
    assert_eq!(
        RetryDelay::random(Duration::from_millis(7), Duration::from_millis(7)).base_delay(1),
        Duration::from_millis(7)
    );

    let random = RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8));
    for _ in 0..20 {
        let delay = random.base_delay(1);
        assert!(delay >= Duration::from_millis(5));
        assert!(delay <= Duration::from_millis(8));
    }

    let exponential =
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0);
    assert_eq!(exponential.base_delay(0), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(1), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(2), Duration::from_millis(200));
    assert_eq!(exponential.base_delay(4), Duration::from_millis(500));
    assert_eq!(exponential.base_delay(u32::MAX), Duration::from_millis(500));
}

/// Verifies exponential delay handles very large durations without lossy
/// nanosecond downcasts.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when large-duration exponential delay
/// calculation truncates unexpectedly.
#[test]
fn test_exponential_delay_handles_large_durations() {
    let initial = Duration::from_secs(20_000_000_000);
    let max = Duration::from_secs(40_000_000_000);
    let exponential = RetryDelay::exponential(initial, max, 2.0);

    assert_eq!(exponential.base_delay(1), initial);
    assert_eq!(exponential.base_delay(2), max);
    assert_eq!(exponential.base_delay(3), max);
}

/// Verifies exponential retry uses attempt index `0` and `1` consistently and
/// stops at the configured cap.
#[test]
fn test_exponential_delay_uses_first_attempt_indices_and_caps_at_max() {
    let exponential =
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(180), 1.7);

    assert_eq!(exponential.base_delay(0), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(1), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(2), Duration::from_millis(170));
    assert_eq!(exponential.base_delay(3), Duration::from_millis(180));
    assert_eq!(exponential.base_delay(4), Duration::from_millis(180));
}

/// Verifies exponential delay is capped by max immediately when any scaling path exceeds it.
#[test]
fn test_exponential_delay_cap_applied_when_scaled_delay_exceeds_max() {
    let exponential =
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(120), 10.0);

    assert_eq!(exponential.base_delay(2), Duration::from_millis(120));
    assert_eq!(exponential.base_delay(3), Duration::from_millis(120));
}

/// Verifies delay validation rejects invalid strategy parameters.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when invalid values are accepted or valid
/// values are rejected.
#[test]
fn test_validate_rejects_invalid_values() {
    assert!(RetryDelay::fixed(Duration::ZERO).validate().is_err());
    assert!(
        RetryDelay::random(Duration::ZERO, Duration::from_millis(1))
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::random(Duration::from_millis(2), Duration::from_millis(1))
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::random(Duration::from_millis(2), Duration::from_millis(2))
            .validate()
            .is_ok()
    );
    assert!(
        RetryDelay::exponential(Duration::ZERO, Duration::from_secs(1), 2.0)
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::exponential(Duration::from_secs(2), Duration::from_secs(1), 2.0)
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::exponential(Duration::from_secs(1), Duration::from_secs(2), 1.0)
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::exponential(
            Duration::from_secs(1),
            Duration::from_secs(2),
            f64::INFINITY
        )
        .validate()
        .is_err()
    );
    assert!(RetryDelay::default().validate().is_ok());
}

/// Verifies JSON serialization and deserialization for [`RetryDelay`] (millisecond
/// fields and `f64` multiplier) round-trip for representative values.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions or serde errors when JSON does not round-trip.
#[test]
fn test_retry_delay_serde_json_roundtrip_variants() {
    let cases = [
        RetryDelay::none(),
        RetryDelay::fixed(Duration::from_millis(12)),
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8)),
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0),
        RetryDelay::default(),
    ];
    for original in cases {
        let json = serde_json::to_string(&original).unwrap();
        let parsed: RetryDelay = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original, "json was {json}");
    }
}

/// Documents stable JSON shapes for [`RetryDelay`] literals used in configuration.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when serialized JSON drifts from expectations.
#[test]
fn test_retry_delay_serde_json_literal_shapes() {
    assert_eq!(
        serde_json::to_string(&RetryDelay::none()).unwrap(),
        r#""None""#
    );
    assert_eq!(
        serde_json::to_string(&RetryDelay::fixed(Duration::from_millis(12))).unwrap(),
        r#"{"Fixed":12}"#
    );
    assert_eq!(
        serde_json::to_string(&RetryDelay::random(
            Duration::from_millis(5),
            Duration::from_millis(8)
        ))
        .unwrap(),
        r#"{"Random":{"min":5,"max":8}}"#
    );
    assert_eq!(
        serde_json::to_string(&RetryDelay::exponential(
            Duration::from_millis(100),
            Duration::from_millis(500),
            2.0
        ))
        .unwrap(),
        r#"{"Exponential":{"initial":100,"max":500,"multiplier":2.0}}"#
    );
}

/// Verifies [`qubit_retry::constants::DEFAULT_RETRY_DELAY`] matches
/// [`RetryDelay::default`].
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the default string and `Default`
/// drift apart.
#[test]
fn test_default_retry_delay_string_matches_retry_delay_default() {
    assert_eq!(
        RetryDelay::from_str(DEFAULT_RETRY_DELAY).unwrap(),
        RetryDelay::default()
    );
    assert_eq!(
        RetryDelay::default(),
        RetryDelay::exponential(
            Duration::from_millis(1000),
            Duration::from_millis(60000),
            2.0
        )
    );
}
