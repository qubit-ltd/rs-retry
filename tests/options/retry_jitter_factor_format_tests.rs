// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::str::FromStr;

use qubit_retry::RetryJitter;

/// Verifies jitter-factor text formatting round-trips through validated
/// parsing.
#[test]
fn test_retry_jitter_factor_format_display_and_parse_round_trip() {
    let jitter = RetryJitter::factor(0.25);

    assert_eq!(jitter.to_string(), "factor:0.25");
    assert_eq!(
        RetryJitter::from_str(&jitter.to_string())
            .expect("displayed jitter factor should parse"),
        jitter,
    );
    for invalid in ["factor:-0.1", "factor:1.1", "factor:nan", "factor:inf"] {
        assert!(
            RetryJitter::from_str(invalid).is_err(),
            "invalid factor should be rejected: {invalid}"
        );
    }
}
