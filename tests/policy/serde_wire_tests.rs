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
use qubit_retry::RetryPolicy;
use serde_json::json;
use serde_json::to_value;

/// Serializes the public configuration types with their fixed wire layout.
#[test]
fn test_serde_serializes_configuration_with_stable_golden_json() {
    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .total_time_budget(Duration::from_secs(10))
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
        to_value(&policy).expect("policy should serialize"),
        json!({
            "max_attempts": 4,
            "operation_time_budget": null,
            "total_time_budget": { "seconds": 10, "nanoseconds": 0 },
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
        to_value(policy.admission_limits()).expect("limits should serialize"),
        json!({
            "max_attempts": 4,
            "operation_time_budget": null,
            "total_time_budget": { "seconds": 10, "nanoseconds": 0 }
        }),
    );
}
