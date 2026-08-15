// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable serde wire-format coverage for retry configuration.

#![cfg(feature = "serde")]

use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryLimits;
use qubit_retry::RetryPolicy;
use serde_json::json;

/// Serializes the public configuration types with their fixed wire layout.
#[test]
fn test_serde_serializes_configuration_with_stable_golden_json() {
    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .max_total_elapsed(Duration::from_secs(10))
        .backoff(
            BackoffPolicy::exponential(
                Duration::from_millis(50),
                2.0,
                Duration::from_secs(2),
            )
            .expect("the golden exponential policy should be valid"),
        )
        .build()
        .expect("the golden retry policy should be valid");

    assert_eq!(
        serde_json::to_value(&policy).expect("policy should serialize"),
        json!({
            "max_attempts": 4,
            "max_operation_elapsed": null,
            "max_total_elapsed": { "seconds": 10, "nanoseconds": 0 },
            "backoff": {
                "strategy": {
                    "type": "exponential",
                    "initial": { "seconds": 0, "nanoseconds": 50_000_000 },
                    "multiplier": 2.0,
                    "maximum": { "seconds": 2, "nanoseconds": 0 }
                },
                "jitter": { "type": "none" },
                "retry_after": "at_least_backoff"
            }
        }),
    );
    assert_eq!(
        serde_json::to_value(policy.limits()).expect("limits should serialize"),
        json!({
            "max_attempts": 4,
            "max_operation_elapsed": null,
            "max_total_elapsed": { "seconds": 10, "nanoseconds": 0 }
        }),
    );
}

/// Keeps every base, jitter, and retry-after strategy explicitly tagged.
#[test]
fn test_serde_uses_snake_case_tags_for_all_backoff_strategies() {
    let cases = [
        (
            BackoffPolicy::immediate(),
            json!({
                "strategy": { "type": "immediate" },
                "jitter": { "type": "none" },
                "retry_after": "at_least_backoff"
            }),
        ),
        (
            BackoffPolicy::fixed(Duration::from_secs(1))
                .with_full_jitter()
                .prefer_retry_after(),
            json!({
                "strategy": {
                    "type": "fixed",
                    "delay": { "seconds": 1, "nanoseconds": 0 }
                },
                "jitter": { "type": "full" },
                "retry_after": "prefer_hint"
            }),
        ),
        (
            BackoffPolicy::uniform(
                Duration::from_millis(1),
                Duration::from_secs(2),
            )
            .expect("the golden uniform policy should be valid")
            .with_bounded_jitter(0.5)
            .expect("the golden bounded jitter should be valid")
            .ignore_retry_after(),
            json!({
                "strategy": {
                    "type": "uniform",
                    "minimum": { "seconds": 0, "nanoseconds": 1_000_000 },
                    "maximum": { "seconds": 2, "nanoseconds": 0 }
                },
                "jitter": { "type": "bounded", "ratio": 0.5 },
                "retry_after": "ignore_hint"
            }),
        ),
    ];

    for (policy, expected) in cases {
        assert_eq!(
            serde_json::to_value(&policy)
                .expect("backoff policy should serialize"),
            expected,
        );
    }
}

/// Rejects unknown and invalid data at every stable wire-format layer.
#[test]
fn test_serde_rejects_unknown_fields_and_tags_at_every_wire_layer() {
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

    let mut limits_unknown_field = json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": null
    });
    limits_unknown_field["unexpected"] = json!(true);
    let error = serde_json::from_value::<RetryLimits>(limits_unknown_field)
        .expect_err("retry limits must reject unknown top-level fields");
    assert!(error.to_string().contains("unknown field"));

    let mut backoff_unknown_field = valid.clone();
    backoff_unknown_field["backoff"]["unexpected"] = json!(true);
    let mut strategy_unknown_field = valid.clone();
    strategy_unknown_field["backoff"]["strategy"]["unexpected"] = json!(true);
    let mut jitter_unknown_field = valid.clone();
    jitter_unknown_field["backoff"]["jitter"]["ratio"] = json!(0.5);
    let mut duration_unknown_field = valid.clone();
    duration_unknown_field["backoff"]["strategy"]["delay"]["unexpected"] =
        json!(true);
    let mut unknown_strategy_tag = valid.clone();
    unknown_strategy_tag["backoff"]["strategy"]["type"] = json!("decorrelated");
    let mut unknown_jitter_tag = valid.clone();
    unknown_jitter_tag["backoff"]["jitter"]["type"] = json!("random");

    for (description, data, expected_error) in [
        (
            "backoff top-level field",
            backoff_unknown_field,
            "unknown field",
        ),
        (
            "strategy variant field",
            strategy_unknown_field,
            "unknown field",
        ),
        (
            "jitter variant field",
            jitter_unknown_field,
            "only valid for bounded jitter",
        ),
        ("duration field", duration_unknown_field, "unknown field"),
        ("strategy tag", unknown_strategy_tag, "unknown variant"),
        ("jitter tag", unknown_jitter_tag, "unknown variant"),
    ] {
        let error = match serde_json::from_value::<RetryPolicy>(data) {
            Ok(policy) => {
                panic!("{description} unexpectedly decoded as {policy:?}")
            }
            Err(error) => error,
        };
        assert!(
            error.to_string().contains(expected_error),
            "{description} should fail with {expected_error:?}, got {error}",
        );
    }
}

/// Rejects absent or null jitter ratios unless the bounded variant supplies
/// one.
#[test]
fn test_serde_rejects_null_or_missing_jitter_ratio_by_variant() {
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
    let cases = [
        (
            "none with null ratio",
            json!({ "type": "none", "ratio": null }),
        ),
        (
            "full with null ratio",
            json!({ "type": "full", "ratio": null }),
        ),
        ("bounded without ratio", json!({ "type": "bounded" })),
        (
            "bounded with null ratio",
            json!({ "type": "bounded", "ratio": null }),
        ),
    ];

    for (description, jitter) in cases {
        let mut data = valid.clone();
        data["backoff"]["jitter"] = jitter;
        assert!(
            serde_json::from_value::<RetryPolicy>(data).is_err(),
            "{description} must be rejected",
        );
    }
}

/// Rejects values that violate validated retry configuration invariants.
#[test]
fn test_serde_rejects_invalid_configuration_data() {
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

    let mut invalid_nanoseconds = valid.clone();
    invalid_nanoseconds["backoff"]["strategy"]["delay"]["nanoseconds"] =
        json!(1_000_000_000_u32);
    let error = serde_json::from_value::<RetryPolicy>(invalid_nanoseconds)
        .expect_err("nanoseconds at one second must be rejected");
    assert!(error.to_string().contains("nanoseconds"));

    let mut zero_attempts = valid.clone();
    zero_attempts["max_attempts"] = json!(0);
    let error = serde_json::from_value::<RetryPolicy>(zero_attempts)
        .expect_err("zero max attempts must be rejected");
    assert!(error.to_string().contains("max_attempts"));

    let mut invalid_jitter = valid;
    invalid_jitter["backoff"]["jitter"] = json!({
        "type": "bounded",
        "ratio": 1.5
    });
    let error = serde_json::from_value::<RetryPolicy>(invalid_jitter)
        .expect_err("jitter above one must be rejected");
    assert!(error.to_string().contains("jitter ratio"));

    let error = serde_json::from_value::<RetryLimits>(json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": { "seconds": 0, "nanoseconds": 1_000_000_000 }
    }))
    .expect_err("invalid retry-limit duration must be rejected");
    assert!(error.to_string().contains("nanoseconds"));
}
