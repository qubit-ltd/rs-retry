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
            BackoffPolicy::uniform(Duration::from_millis(1), Duration::from_secs(2))
                .expect("uniform strategy should be valid"),
            json!({
                "type": "uniform",
                "minimum": { "seconds": 0, "nanoseconds": 1_000_000 },
                "maximum": { "seconds": 2, "nanoseconds": 0 }
            }),
        ),
        (
            BackoffPolicy::exponential(Duration::from_millis(50), 2.0, Duration::from_secs(2))
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
        "operation_time_budget": null,
        "total_time_budget": null,
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
    let error = from_value::<RetryPolicy>(unknown_field).expect_err("strategy data must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));

    let mut unknown_tag = valid;
    unknown_tag["backoff"]["strategy"]["type"] = json!("decorrelated");
    let error = from_value::<RetryPolicy>(unknown_tag).expect_err("strategy data must reject unknown tags");
    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn test_immediate_strategy_rejects_unknown_and_unrelated_fields() {
    let valid = json!({
        "strategy": { "type": "immediate" },
        "jitter": { "type": "none" },
        "retry_after": "at_least_backoff"
    });
    let policy = from_value::<BackoffPolicy>(valid.clone()).expect("immediate strategy should deserialize");
    assert_eq!(to_value(&policy).expect("immediate policy should serialize"), valid,);
    assert_eq!(
        from_value::<BackoffPolicy>(to_value(policy).expect("policy should serialize"))
            .expect("immediate policy should round-trip"),
        BackoffPolicy::immediate(),
    );

    for extra in [
        json!(true),
        json!({ "seconds": 60, "nanoseconds": 0 }),
        json!({ "seconds": 0, "nanoseconds": 0 }),
    ] {
        let mut invalid = valid.clone();
        let key = if extra.is_boolean() { "unexpected" } else { "delay" };
        invalid["strategy"][key] = extra;
        assert!(
            from_value::<BackoffPolicy>(invalid).is_err(),
            "immediate strategy should reject {key}"
        );
    }

    let retry_policy = json!({
        "max_attempts": 4,
        "operation_time_budget": null,
        "total_time_budget": null,
        "backoff": valid
    });
    let mut invalid_retry_policy = retry_policy;
    invalid_retry_policy["backoff"]["strategy"]["unexpected"] = json!(true);
    assert!(from_value::<RetryPolicy>(invalid_retry_policy).is_err());
}
