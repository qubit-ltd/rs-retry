// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::BackoffPolicy;

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_round_trip_preserves_valid_policy() {
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(10),
        2.0,
        Duration::from_secs(1),
    )
    .unwrap()
    .with_bounded_jitter(0.25)
    .unwrap()
    .prefer_retry_after();
    let encoded =
        serde_json::to_value(&policy).expect("policy should serialize");
    let decoded: BackoffPolicy =
        serde_json::from_value(encoded).expect("policy should decode");
    assert_eq!(decoded, policy);
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_uniform_bounds() {
    let policy =
        BackoffPolicy::uniform(Duration::from_secs(1), Duration::from_secs(2))
            .unwrap();
    let mut encoded = serde_json::to_value(policy).unwrap();
    encoded["strategy"]["minimum"] =
        serde_json::json!({"seconds": 3, "nanoseconds": 0});
    let error = serde_json::from_value::<BackoffPolicy>(encoded)
        .expect_err("reversed uniform bounds must be rejected");
    assert!(error.to_string().contains("minimum delay"));
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_exponential_values() {
    let policy = BackoffPolicy::exponential(
        Duration::from_secs(1),
        2.0,
        Duration::from_secs(2),
    )
    .unwrap();
    let mut reversed = serde_json::to_value(&policy).unwrap();
    reversed["strategy"]["initial"] = serde_json::json!({
        "seconds": 3,
        "nanoseconds": 0
    });
    let error = serde_json::from_value::<BackoffPolicy>(reversed)
        .expect_err("an initial delay above maximum must be rejected");
    assert!(error.to_string().contains("initial delay"));

    let mut multiplier = serde_json::to_value(policy).unwrap();
    multiplier["strategy"]["multiplier"] = serde_json::json!(0.5);
    let error = serde_json::from_value::<BackoffPolicy>(multiplier)
        .expect_err("a multiplier below one must be rejected");
    assert!(error.to_string().contains("multiplier"));
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_jitter_ratio() {
    let policy = BackoffPolicy::fixed(Duration::from_secs(1))
        .with_bounded_jitter(0.25)
        .unwrap();
    let mut encoded = serde_json::to_value(policy).unwrap();
    encoded["jitter"]["ratio"] = serde_json::json!(1.5);
    let error = serde_json::from_value::<BackoffPolicy>(encoded)
        .expect_err("a jitter ratio above one must be rejected");
    assert!(error.to_string().contains("jitter ratio"));
}

#[test]
fn test_exponential_rejects_invalid_values() {
    assert!(
        BackoffPolicy::exponential(
            Duration::from_millis(10),
            f64::NAN,
            Duration::from_secs(1),
        )
        .is_err()
    );
    assert!(
        BackoffPolicy::exponential(
            Duration::from_secs(2),
            2.0,
            Duration::from_secs(1),
        )
        .is_err()
    );
}

#[test]
fn test_uniform_rejects_reversed_bounds() {
    assert!(
        BackoffPolicy::uniform(Duration::from_secs(2), Duration::from_secs(1),)
            .is_err()
    );
}
