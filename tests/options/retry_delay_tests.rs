/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
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

/// Verifies [`std::fmt::Display`] for every [`RetryDelay`] variant.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the display string is incorrect.
#[test]
fn test_retry_delay_display_variants() {
    assert_eq!(RetryDelay::none().to_string(), "none");
    assert_eq!(
        RetryDelay::fixed(Duration::from_millis(12)).to_string(),
        "fixed(12ms)"
    );
    assert_eq!(
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8)).to_string(),
        "random(5ms..=8ms)"
    );
    assert_eq!(
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0)
            .to_string(),
        "exponential(initial=100ms, max=500ms, multiplier=2)"
    );
}

/// Verifies [`std::str::FromStr`] for every [`RetryDelay`] variant.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when parsing does not match the expected rules.
#[test]
fn test_retry_delay_from_str_variants() {
    assert_eq!(RetryDelay::from_str("none").unwrap(), RetryDelay::none());
    assert_eq!(
        RetryDelay::from_str("fixed(12ms)").unwrap(),
        RetryDelay::fixed(Duration::from_millis(12))
    );
    assert_eq!(
        RetryDelay::from_str("random(5ms..=8ms)").unwrap(),
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8))
    );
    assert_eq!(
        RetryDelay::from_str("exponential(initial=100ms, max=500ms, multiplier=2)").unwrap(),
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0)
    );
    assert_eq!(
        RetryDelay::from_str("exponential(initial=100ms, max=500ms, multiplier=2.0)").unwrap(),
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0)
    );
}

/// Verifies [`std::str::FromStr`] rejects malformed retry-delay strings.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when invalid input is accepted.
#[test]
fn test_retry_delay_from_str_rejects_invalid_inputs() {
    for s in [
        "",
        " ",
        "  none  ",
        "NONE",
        "None",
        "nope",
        "fixed",
        "fixed(12)",
        "fixed(ms)",
        "random(5..=8)",
        "random(5ms..8ms)",
        "exponential(initial=100ms,max=500ms,multiplier=2)",
        "exponential(initial=100ms, max=500ms, multiplier=)",
    ] {
        assert!(
            RetryDelay::from_str(s).is_err(),
            "expected from_str error for {s:?}"
        );
    }
}

/// Verifies display → parse round-trip for representative [`RetryDelay`] values.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the round-trip does not hold.
#[test]
fn test_retry_delay_display_parse_round_trip_variants() {
    let cases = [
        RetryDelay::none(),
        RetryDelay::fixed(Duration::from_millis(12)),
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8)),
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0),
    ];
    for delay in cases {
        let s = delay.to_string();
        assert_eq!(RetryDelay::from_str(&s).unwrap(), delay);
    }
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
