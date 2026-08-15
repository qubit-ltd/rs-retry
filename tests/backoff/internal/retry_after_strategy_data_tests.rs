// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public retry-after wire behavior.

#![cfg(feature = "serde")]

use qubit_retry::BackoffPolicy;
use serde_json::to_value;

#[test]
fn test_retry_after_strategy_data_serializes_every_public_tag() {
    let cases = [
        (BackoffPolicy::immediate(), "at_least_backoff"),
        (
            BackoffPolicy::immediate().prefer_retry_after(),
            "prefer_hint",
        ),
        (
            BackoffPolicy::immediate().ignore_retry_after(),
            "ignore_hint",
        ),
    ];
    for (policy, expected) in cases {
        assert_eq!(to_value(policy).unwrap()["retry_after"], expected);
    }
}
