// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-limit wire behavior.

#![cfg(feature = "serde")]

use qubit_retry::RetryLimits;
use serde_json::from_value;
use serde_json::json;

#[test]
fn test_retry_limits_data_rejects_invalid_attempts_duration_and_fields() {
    let zero_attempts = json!({
        "max_attempts": 0,
        "max_operation_elapsed": null,
        "max_total_elapsed": null
    });
    let error = from_value::<RetryLimits>(zero_attempts)
        .expect_err("zero max attempts must be rejected");
    assert!(error.to_string().contains("max_attempts"));

    let invalid_duration = json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": { "seconds": 0, "nanoseconds": 1_000_000_000 }
    });
    let error = from_value::<RetryLimits>(invalid_duration)
        .expect_err("invalid retry-limit duration must be rejected");
    assert!(error.to_string().contains("nanoseconds"));

    let unknown_field = json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": null,
        "unexpected": true
    });
    let error = from_value::<RetryLimits>(unknown_field)
        .expect_err("retry limits must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));
}
