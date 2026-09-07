// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public wire behavior owned by backoff-policy data.

#![cfg(feature = "serde")]

use qubit_retry::RetryPolicy;
use serde_json::from_value;
use serde_json::json;

/// Verifies the nested backoff policy object rejects unknown fields.
#[test]
fn test_backoff_policy_data_rejects_unknown_fields() {
    let error = from_value::<RetryPolicy>(json!({
        "max_attempts": 4,
        "operation_time_budget": null,
        "total_time_budget": null,
        "backoff": {
            "strategy": { "type": "immediate" },
            "jitter": { "type": "none" },
            "retry_after": "at_least_backoff",
            "unexpected": true
        }
    }))
    .expect_err("backoff data must reject unknown fields");
    assert!(error.to_string().contains("unknown field"));
}
