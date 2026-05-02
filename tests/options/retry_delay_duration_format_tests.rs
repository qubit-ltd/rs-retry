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
        RetryDelay::from_str("fixed(12)").unwrap(),
        RetryDelay::fixed(Duration::from_millis(12))
    );
    assert_eq!(
        RetryDelay::from_str("fixed(1s)").unwrap(),
        RetryDelay::fixed(Duration::from_secs(1))
    );
    assert_eq!(
        RetryDelay::from_str("random(5ms..=8ms)").unwrap(),
        RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8))
    );
    assert_eq!(
        RetryDelay::from_str("random(5..=8)").unwrap(),
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
        "fixed(ms)",
        "fixed(18446744073709551616ms)",
        "random(5ms..8ms)",
        "random(5ms..=18446744073709551616ms)",
        "exponential(initial=100ms,max=500ms,multiplier=2)",
        "exponential(initial=100ms, max=500ms, multiplier=)",
        "exponential(initial=18446744073709551616ms, max=500ms, multiplier=2)",
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
