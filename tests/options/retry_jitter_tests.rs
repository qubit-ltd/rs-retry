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

use qubit_retry::constants::DEFAULT_RETRY_JITTER;
use qubit_retry::{RetryDelay, RetryJitter};

/// Verifies factor jitter application and validation bounds.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when jitter output or validation behavior
/// is incorrect.
#[test]
fn test_apply_symmetric_factor_and_validate_bounds() {
    let base = Duration::from_millis(100);
    assert_eq!(RetryJitter::none().apply(base), base);
    assert_eq!(RetryJitter::factor(0.0).apply(base), base);
    assert_eq!(
        RetryJitter::factor(0.5).apply(Duration::ZERO),
        Duration::ZERO
    );
    assert_eq!(RetryJitter::default(), RetryJitter::None);

    for _ in 0..30 {
        let delay = RetryJitter::factor(0.2).apply(base);
        assert!(delay >= Duration::from_millis(80));
        assert!(delay <= Duration::from_millis(120));
    }

    assert!(RetryJitter::factor(0.0).validate().is_ok());
    assert!(RetryJitter::factor(1.0).validate().is_ok());
    assert!(RetryJitter::factor(-0.1).validate().is_err());
    assert!(RetryJitter::factor(1.1).validate().is_err());
    assert!(RetryJitter::factor(f64::NAN).validate().is_err());
}

/// Verifies invalid jitter factors do not panic when applied directly.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when invalid factors do not gracefully
/// degrade to the base delay.
#[test]
fn test_apply_invalid_factor_falls_back_to_base_delay() {
    let base = Duration::from_millis(100);
    assert_eq!(RetryJitter::factor(f64::NAN).apply(base), base);
    assert_eq!(RetryJitter::factor(f64::INFINITY).apply(base), base);
    assert_eq!(RetryJitter::factor(f64::NEG_INFINITY).apply(base), base);
}

/// Verifies jitter preserves very large durations that exceed `u64` nanoseconds.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when jitter application truncates huge
/// durations.
#[test]
fn test_apply_large_duration_factor_returns_base() {
    let base = Duration::from_secs(20_000_000_000);
    assert_eq!(RetryJitter::factor(0.5).apply(base), base);
}

/// Verifies `delay_for_attempt` combines base-delay strategy and jitter.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when composed delay calculation is
/// incorrect.
#[test]
fn test_delay_for_attempt_combines_delay_strategy_and_jitter() {
    let fixed = RetryDelay::fixed(Duration::from_millis(50));
    assert_eq!(
        RetryJitter::none().delay_for_attempt(&fixed, 1),
        Duration::from_millis(50)
    );

    let exponential =
        RetryDelay::exponential(Duration::from_millis(10), Duration::from_millis(80), 2.0);
    assert_eq!(
        RetryJitter::none().delay_for_attempt(&exponential, 1),
        Duration::from_millis(10)
    );
    assert_eq!(
        RetryJitter::none().delay_for_attempt(&exponential, 4),
        Duration::from_millis(80)
    );

    for _ in 0..30 {
        let delay = RetryJitter::factor(0.2).delay_for_attempt(&fixed, 2);
        assert!(delay >= Duration::from_millis(40));
        assert!(delay <= Duration::from_millis(60));
    }
}

/// Documents [`std::str::FromStr`] for [`RetryJitter`].
///
/// Accepts `none` (ASCII case-insensitive) or `factor:<f64>` with ASCII trimming
/// around the number; the factor must lie in `[0.0, 1.0]`.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when parsing behavior changes unexpectedly.
#[test]
fn test_retry_jitter_from_str() {
    assert_eq!(RetryJitter::from_str("none").unwrap(), RetryJitter::None);
    assert_eq!(
        RetryJitter::from_str("  none  ").unwrap(),
        RetryJitter::None
    );
    assert_eq!(RetryJitter::from_str("NONE").unwrap(), RetryJitter::None);
    assert_eq!(
        RetryJitter::from_str("factor:0.2").unwrap(),
        RetryJitter::factor(0.2)
    );
    assert_eq!(
        RetryJitter::from_str("factor: 0.25 ").unwrap(),
        RetryJitter::factor(0.25)
    );
    assert!(RetryJitter::from_str("factor").is_err());
    assert!(RetryJitter::from_str("factor()").is_err());
    assert!(RetryJitter::from_str("factor(0.2)").is_err());
    assert_eq!(
        RetryJitter::from_str("factor:1.1").unwrap_err().to_string(),
        "parse failed."
    );
    assert_eq!(
        RetryJitter::from_str("factor:-0.1")
            .unwrap_err()
            .to_string(),
        "parse failed."
    );
    assert!(RetryJitter::from_str("factor:").is_err());
    assert!(RetryJitter::from_str("").is_err());
}

/// Covers additional [`RetryJitter::from_str`] branches: boundaries, numeric forms,
/// ASCII `none` spellings, and case-sensitive `factor:` prefix.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when parsing behavior changes unexpectedly.
#[test]
fn test_retry_jitter_from_str_boundaries_and_numeric_forms() {
    assert_eq!(
        RetryJitter::from_str("factor:0").unwrap(),
        RetryJitter::factor(0.0)
    );
    assert_eq!(
        RetryJitter::from_str("factor:1").unwrap(),
        RetryJitter::factor(1.0)
    );
    assert_eq!(
        RetryJitter::from_str("factor:1.0").unwrap(),
        RetryJitter::factor(1.0)
    );
    assert_eq!(
        RetryJitter::from_str("factor:0.0").unwrap(),
        RetryJitter::factor(0.0)
    );
    assert_eq!(
        RetryJitter::from_str("factor:.5").unwrap(),
        RetryJitter::factor(0.5)
    );
    assert_eq!(
        RetryJitter::from_str("factor:+0.25").unwrap(),
        RetryJitter::factor(0.25)
    );
    assert_eq!(
        RetryJitter::from_str("factor:1e0").unwrap(),
        RetryJitter::factor(1.0)
    );
    assert_eq!(
        RetryJitter::from_str("factor:5e-1").unwrap(),
        RetryJitter::factor(0.5)
    );

    assert_eq!(
        RetryJitter::from_str("  \t factor:0.3 \n ").unwrap(),
        RetryJitter::factor(0.3)
    );
    assert_eq!(RetryJitter::from_str("NoNe").unwrap(), RetryJitter::None);

    for prefix in ["FACTOR:0.5", "Factor:0.5", "not-factor:0.5"] {
        assert!(
            RetryJitter::from_str(prefix).is_err(),
            "expected parse error for {prefix:?}"
        );
    }

    assert_eq!(
        RetryJitter::from_str("  factor:0.4  ").unwrap(),
        RetryJitter::factor(0.4)
    );
}

/// Verifies [`RetryJitter::from_str`] rejects empty / non-matching tokens and values
/// outside `[0.0, 1.0]` including non-finite floats parsed from text.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when parsing behavior changes unexpectedly.
#[test]
fn test_retry_jitter_from_str_invalid_format_out_of_range_and_bad_number() {
    for s in ["", "   ", "other", "nonee", "fact", "factor"] {
        assert!(
            RetryJitter::from_str(s).is_err(),
            "expected parse error for {s:?}"
        );
    }

    assert_eq!(
        RetryJitter::from_str("factor:2").unwrap_err().to_string(),
        "parse failed."
    );
    assert_eq!(
        RetryJitter::from_str("factor:-1").unwrap_err().to_string(),
        "parse failed."
    );

    for s in ["factor:nan", "factor:inf", "factor:Infinity"] {
        let err = RetryJitter::from_str(s).unwrap_err();
        assert_eq!(
            err.to_string(),
            "parse failed.",
            "expected non-finite range error for {s:?}"
        );
    }

    assert!(RetryJitter::from_str("factor:  ").is_err());
    assert_eq!(
        RetryJitter::from_str("factor:xyz").unwrap_err().to_string(),
        "parse failed."
    );
}

/// Verifies [`ParseRetryJitterError`] [`std::fmt::Display`] text and [`Error::source`]
/// behavior.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when error representations change unexpectedly.
#[test]
fn test_parse_retry_jitter_error_display_and_source() {
    let invalid_format = RetryJitter::from_str("nope").unwrap_err();
    assert_eq!(invalid_format.to_string(), "parse failed.");
    assert!(std::error::Error::source(&invalid_format).is_none());

    let out_of_range = RetryJitter::from_str("factor:3").unwrap_err();
    assert_eq!(out_of_range.to_string(), "parse failed.");
    assert!(std::error::Error::source(&out_of_range).is_none());

    let bad_number = RetryJitter::from_str("factor:not-a-number").unwrap_err();
    assert_eq!(bad_number.to_string(), "parse failed.");
    assert!(std::error::Error::source(&bad_number).is_none());
}

/// Verifies [`std::fmt::Display`] / [`std::str::FromStr`] round-trip for edge factors
/// and stable parsing of [`RetryJitter`] display output.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when display or parsing behavior drifts.
#[test]
fn test_retry_jitter_display_parse_round_trip_variants() {
    for jitter in [
        RetryJitter::None,
        RetryJitter::factor(0.0),
        RetryJitter::factor(1.0),
        RetryJitter::factor(0.125),
    ] {
        let s = jitter.to_string();
        assert_eq!(RetryJitter::from_str(&s).unwrap(), jitter);
    }
}

/// Documents [`std::fmt::Display`] and display / parse round-trip for [`RetryJitter`].
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when display behavior changes unexpectedly.
#[test]
fn test_retry_jitter_display_and_round_trip() {
    assert_eq!(format!("{}", RetryJitter::None), "none");
    assert_eq!(format!("{}", RetryJitter::none()), "none");
    assert_eq!(format!("{}", RetryJitter::factor(0.25)), "factor:0.25");

    let parsed = RetryJitter::from_str(&format!("{}", RetryJitter::factor(0.25))).unwrap();
    assert_eq!(parsed, RetryJitter::factor(0.25));
}

/// Documents JSON shape produced by `serde_json` for [`RetryJitter`].
///
/// `serde_json` encodes a **unit** enum variant as a bare JSON string holding the
/// Rust variant name (for example [`RetryJitter::None`] becomes `"None"`). A
/// **single-field** tuple variant is encoded as a one-key object mapping the
/// variant name to the inner value (for example `{"Factor":0.25}`).
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when serde JSON encoding changes unexpectedly.
#[test]
fn test_retry_jitter_json_serde_json_shapes() {
    assert_eq!(
        serde_json::to_string(&RetryJitter::None).unwrap(),
        r#""None""#
    );
    assert_eq!(
        serde_json::to_string(&RetryJitter::factor(0.25)).unwrap(),
        r#"{"Factor":0.25}"#
    );

    assert_eq!(
        serde_json::from_str::<RetryJitter>(r#""None""#).unwrap(),
        RetryJitter::None
    );
    assert_eq!(
        serde_json::from_str::<RetryJitter>(r#"{"Factor":0.25}"#).unwrap(),
        RetryJitter::factor(0.25)
    );
}

/// Verifies [`qubit_retry::constants::DEFAULT_RETRY_JITTER`] matches [`RetryJitter::default`].
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the default string and `Default` drift apart.
#[test]
fn test_default_retry_jitter_string_matches_retry_jitter_default() {
    assert_eq!(
        RetryJitter::from_str(DEFAULT_RETRY_JITTER).unwrap(),
        RetryJitter::default()
    );
    assert_eq!(RetryJitter::default(), RetryJitter::None);
}
