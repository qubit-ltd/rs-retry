// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::str::FromStr;

use qubit_retry::RetryJitter;

/// Verifies retry-jitter parse diagnostics and source behavior.
#[test]
fn test_parse_retry_jitter_error_display_and_source() {
    let invalid_format = RetryJitter::from_str("nope").unwrap_err();
    assert_eq!(
        invalid_format.to_string(),
        "invalid retry jitter: expected 'none' or 'factor:<f64>'"
    );
    assert!(std::error::Error::source(&invalid_format).is_none());

    let out_of_range = RetryJitter::from_str("factor:3").unwrap_err();
    assert_eq!(
        out_of_range.to_string(),
        "invalid retry jitter factor: expected a finite value in [0.0, 1.0]"
    );
    assert!(std::error::Error::source(&out_of_range).is_none());

    let bad_number = RetryJitter::from_str("factor:not-a-number").unwrap_err();
    assert_eq!(
        bad_number.to_string(),
        "invalid retry jitter factor: expected a floating-point number"
    );
    assert!(std::error::Error::source(&bad_number).is_none());
}
