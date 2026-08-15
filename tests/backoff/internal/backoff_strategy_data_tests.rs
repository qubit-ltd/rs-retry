// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public base-delay strategy wire behavior.

#![cfg(feature = "serde")]

use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;
use serde_json::from_value;
use serde_json::json;
use serde_json::to_value;

#[test]
fn test_backoff_strategy_data_serializes_every_public_variant() {
    let cases = [
        (BackoffPolicy::immediate(), json!({ "type": "immediate" })),
        (
            BackoffPolicy::fixed(Duration::from_millis(25)),
            json!({
                "type": "fixed",
                "delay": { "seconds": 0, "nanoseconds": 25_000_000 }
            }),
        ),
        (
            BackoffPolicy::uniform(
                Duration::from_millis(1),
                Duration::from_secs(2),
            )
            .expect("uniform strategy should be valid"),
            json!({
                "type": "uniform",
                "minimum": { "seconds": 0, "nanoseconds": 1_000_000 },
                "maximum": { "seconds": 2, "nanoseconds": 0 }
            }),
        ),
        (
            BackoffPolicy::exponential(
                Duration::from_millis(50),
                2.0,
                Duration::from_secs(2),
            )
            .expect("exponential strategy should be valid"),
            json!({
                "type": "exponential",
                "initial": { "seconds": 0, "nanoseconds": 50_000_000 },
                "multiplier": 2.0,
                "maximum": { "seconds": 2, "nanoseconds": 0 }
            }),
        ),
    ];
    for (policy, expected) in cases {
        let value = to_value(policy).expect("policy should serialize");
        assert_eq!(value["strategy"], expected);
    }
}

/// Verifies strategy data rejects unknown fields and variant tags.
#[test]
fn test_backoff_strategy_data_rejects_unknown_fields_and_tags() {
    let valid = json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": null,
        "backoff": {
            "strategy": {
                "type": "fixed",
                "delay": { "seconds": 1, "nanoseconds": 0 }
            },
            "jitter": { "type": "none" },
            "retry_after": "at_least_backoff"
        }
    });
    let mut unknown_field = valid.clone();
    unknown_field["backoff"]["strategy"]["unexpected"] = json!(true);
    let error = from_value::<RetryPolicy>(unknown_field)
        .expect_err("strategy data must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));

    let mut unknown_tag = valid;
    unknown_tag["backoff"]["strategy"]["type"] = json!("decorrelated");
    let error = from_value::<RetryPolicy>(unknown_tag)
        .expect_err("strategy data must reject unknown tags");
    assert!(error.to_string().contains("unknown variant"));
}
