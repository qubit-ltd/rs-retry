// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public jitter-strategy wire behavior.

#![cfg(feature = "serde")]

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;
use serde_json::from_value;
use serde_json::json;
use serde_json::to_value;

#[test]
fn test_jitter_strategy_data_serializes_variants_and_rejects_invalid_ratio_shapes()
 {
    let values = [
        (BackoffPolicy::immediate(), json!({ "type": "none" })),
        (
            BackoffPolicy::immediate().with_full_jitter(),
            json!({ "type": "full" }),
        ),
        (
            BackoffPolicy::immediate()
                .with_bounded_jitter(0.5)
                .expect("bounded jitter should be valid"),
            json!({ "type": "bounded", "ratio": 0.5 }),
        ),
    ];
    for (policy, expected) in values {
        assert_eq!(to_value(policy).unwrap()["jitter"], expected);
    }
    let valid = json!({"max_attempts": 4, "max_operation_elapsed": null, "max_total_elapsed": null, "backoff": {"strategy": {"type": "fixed", "delay": {"seconds": 1, "nanoseconds": 0}}, "jitter": {"type": "none"}, "retry_after": "at_least_backoff"}});
    for (jitter, expected_error) in [
        (json!({"type": "none", "ratio": null}), "expected f64"),
        (
            json!({"type": "none", "ratio": 0.5}),
            "only valid for bounded jitter",
        ),
        (json!({"type": "full", "ratio": null}), "expected f64"),
        (
            json!({"type": "full", "ratio": 0.5}),
            "only valid for bounded jitter",
        ),
        (json!({"type": "bounded"}), "bounded jitter requires ratio"),
        (json!({"type": "bounded", "ratio": null}), "expected f64"),
        (json!({"type": "bounded", "ratio": 1.5}), "jitter ratio"),
    ] {
        let mut data = valid.clone();
        data["backoff"]["jitter"] = jitter;
        let error = from_value::<RetryPolicy>(data)
            .expect_err("invalid jitter data must be rejected");
        assert!(
            error.to_string().contains(expected_error),
            "jitter error should contain {expected_error:?}: {error}",
        );
    }
    let mut unknown_tag = valid;
    unknown_tag["backoff"]["jitter"]["type"] = json!("random");
    let error = from_value::<RetryPolicy>(unknown_tag)
        .expect_err("jitter data must reject unknown tags");
    assert!(error.to_string().contains("unknown variant"));
}
