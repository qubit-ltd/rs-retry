// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public duration wire behavior.

#![cfg(feature = "serde")]

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;
use serde_json::from_value;
use serde_json::json;
use serde_json::to_value;

#[test]
fn test_duration_data_uses_wire_components_and_rejects_invalid_nanoseconds() {
    let value =
        to_value(BackoffPolicy::fixed(std::time::Duration::from_nanos(1)))
            .expect("policy should serialize");
    assert_eq!(value["strategy"]["delay"]["seconds"], 0);
    assert_eq!(value["strategy"]["delay"]["nanoseconds"], 1);
    let invalid = json!({"max_attempts": 4, "max_operation_elapsed": null, "max_total_elapsed": null, "backoff": {"strategy": {"type": "fixed", "delay": {"seconds": 1, "nanoseconds": 1_000_000_000}}, "jitter": {"type": "none"}, "retry_after": "at_least_backoff"}});
    let error = from_value::<RetryPolicy>(invalid)
        .expect_err("invalid nanoseconds must fail");
    assert!(error.to_string().contains("nanoseconds"));

    let unknown_field = json!({
        "max_attempts": 4,
        "max_operation_elapsed": null,
        "max_total_elapsed": null,
        "backoff": {
            "strategy": {
                "type": "fixed",
                "delay": {
                    "seconds": 1,
                    "nanoseconds": 0,
                    "unexpected": true
                }
            },
            "jitter": { "type": "none" },
            "retry_after": "at_least_backoff"
        }
    });
    let error = from_value::<RetryPolicy>(unknown_field)
        .expect_err("duration data must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));
}
