// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-policy wire behavior.

#![cfg(feature = "serde")]

use qubit_retry::RetryPolicy;
use serde_json::from_value;
use serde_json::json;

#[test]
fn test_retry_policy_data_rejects_unknown_public_wire_fields() {
    let error = from_value::<RetryPolicy>(
        json!({"max_attempts": 1, "max_operation_elapsed": null, "max_total_elapsed": null, "backoff": {"strategy": {"type": "immediate"}, "jitter": {"type": "none"}, "retry_after": "at_least_backoff"}, "unexpected": true}),
    )
    .expect_err("retry policy data must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));
}
