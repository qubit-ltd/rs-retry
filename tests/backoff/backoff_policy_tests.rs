// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
#[cfg(feature = "serde")]
use serde_json::from_value;
#[cfg(feature = "serde")]
use serde_json::json;
#[cfg(feature = "serde")]
use serde_json::to_value;

use crate::support::FixedRetryRandomSource;

/// Inclusive random endpoints must remain exact even beyond f64 precision.
#[test]
fn test_regression_uniform_preserves_large_duration_endpoints() {
    for min in [Duration::ZERO, Duration::new(1 << 20, 123_456_789)] {
        for max in [
            Duration::new(1 << 24, 999_999_999),
            Duration::new(1 << 40, 999_999_999),
            Duration::MAX,
        ] {
            for sample in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let step = BackoffPolicy::uniform(min, max)
                    .expect("ordered bounds")
                    .start_with_random_source(Arc::new(FixedRetryRandomSource::new(sample)))
                    .next(BackoffRequest::policy());
                let actual = step.effective_delay();
                assert!(
                    (min..=max).contains(&actual),
                    "{min:?}..={max:?}, sample={sample}: {actual:?}"
                );
                if sample == 0.0 {
                    assert_eq!(actual, min);
                }
                if sample == 1.0 {
                    assert_eq!(actual, max);
                }
            }
        }
    }
}

/// Zero bounded jitter must preserve every representable Duration exactly.
#[test]
fn test_bounded_zero_jitter_preserves_large_duration_exactly() {
    let initial = Duration::new(9_007_199_254_740_992, 123);
    let actual = BackoffPolicy::fixed(initial)
        .with_bounded_jitter(0.0)
        .expect("zero jitter is valid")
        .start()
        .next(BackoffRequest::policy())
        .effective_delay();

    assert_eq!(actual, initial);
}

/// The first exponential retry must use the configured initial Duration
/// exactly.
#[test]
fn test_exponential_first_retry_preserves_large_initial_duration_exactly() {
    let initial = Duration::new(9_007_199_254_740_992, 123);
    let actual = BackoffPolicy::exponential(initial, 2.0, Duration::MAX)
        .expect("valid exponential policy")
        .start()
        .next(BackoffRequest::policy())
        .effective_delay();

    assert_eq!(actual, initial);
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_round_trip_preserves_valid_policy() {
    let policy = BackoffPolicy::exponential(Duration::from_millis(10), 2.0, Duration::from_secs(1))
        .unwrap()
        .with_bounded_jitter(0.25)
        .unwrap()
        .prefer_retry_after();
    let encoded = to_value(&policy).expect("policy should serialize");
    let decoded: BackoffPolicy = from_value(encoded).expect("policy should decode");
    assert_eq!(decoded, policy);
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_uniform_bounds() {
    let policy = BackoffPolicy::uniform(Duration::from_secs(1), Duration::from_secs(2)).unwrap();
    let mut encoded = to_value(policy).unwrap();
    encoded["strategy"]["minimum"] = json!({"seconds": 3, "nanoseconds": 0});
    let error = from_value::<BackoffPolicy>(encoded).expect_err("reversed uniform bounds must be rejected");
    assert!(error.to_string().contains("minimum delay"));
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_exponential_values() {
    let policy = BackoffPolicy::exponential(Duration::from_secs(1), 2.0, Duration::from_secs(2)).unwrap();
    let mut reversed = to_value(&policy).unwrap();
    reversed["strategy"]["initial"] = json!({
        "seconds": 3,
        "nanoseconds": 0
    });
    let error = from_value::<BackoffPolicy>(reversed).expect_err("an initial delay above maximum must be rejected");
    assert!(error.to_string().contains("initial delay"));

    let mut multiplier = to_value(policy).unwrap();
    multiplier["strategy"]["multiplier"] = json!(0.5);
    let error = from_value::<BackoffPolicy>(multiplier).expect_err("a multiplier below one must be rejected");
    assert!(error.to_string().contains("multiplier"));
}

#[test]
#[cfg(feature = "serde")]
fn test_policy_serde_rejects_invalid_jitter_ratio() {
    let policy = BackoffPolicy::fixed(Duration::from_secs(1))
        .with_bounded_jitter(0.25)
        .unwrap();
    let mut encoded = to_value(policy).unwrap();
    encoded["jitter"]["ratio"] = json!(1.5);
    let error = from_value::<BackoffPolicy>(encoded).expect_err("a jitter ratio above one must be rejected");
    assert!(error.to_string().contains("jitter ratio"));
}

#[test]
fn test_exponential_rejects_invalid_values() {
    assert!(BackoffPolicy::exponential(Duration::from_millis(10), f64::NAN, Duration::from_secs(1),).is_err());
    assert!(BackoffPolicy::exponential(Duration::from_secs(2), 2.0, Duration::from_secs(1),).is_err());
}

#[test]
fn test_uniform_rejects_reversed_bounds() {
    assert!(BackoffPolicy::uniform(Duration::from_secs(2), Duration::from_secs(1),).is_err());
}

/// A wire-configured final delay limit caps jitter and server hints alike.
#[test]
#[cfg(feature = "serde")]
fn test_final_delay_limit_caps_resolved_delay() {
    let base = BackoffPolicy::fixed(Duration::from_secs(10))
        .with_bounded_jitter(0.5)
        .expect("valid jitter");
    let mut wire = to_value(base).expect("serialize base");
    wire["delay_limit"] = json!({"seconds": 10, "nanoseconds": 0});
    let policy: BackoffPolicy = from_value(wire).expect("final limit is supported");
    let mut state = policy.start_with_random_source(Arc::new(crate::support::FixedRetryRandomSource::new(1.0)));
    for request in [
        BackoffRequest::policy(),
        BackoffRequest::hint(Duration::from_secs(100)),
        BackoffRequest::jittered_hint(Duration::from_secs(100)),
    ] {
        assert_eq!(state.next(request).effective_delay(), Duration::from_secs(10));
    }
}

/// Final limits do not change the documented base-policy maximum.
#[test]
fn test_delay_limit_is_independent_of_base_maximum() {
    let policy = BackoffPolicy::fixed(Duration::from_secs(3)).limit_delay(Duration::ZERO);
    assert_eq!(policy.maximum_delay(), Some(Duration::from_secs(3)));
    assert_eq!(policy.delay_limit(), Some(Duration::ZERO));
    assert_eq!(
        policy
            .start()
            .next(BackoffRequest::hint(Duration::MAX))
            .effective_delay(),
        Duration::ZERO
    );
}

/// Wire limits preserve exact nanoseconds and validate the duration
/// representation.
#[test]
#[cfg(feature = "serde")]
fn test_delay_limit_serde_roundtrip_and_validation() {
    let policy = BackoffPolicy::immediate().limit_delay(Duration::from_nanos(7));
    let mut wire = to_value(&policy).expect("serialize cap");
    assert_eq!(from_value::<BackoffPolicy>(wire.clone()).expect("decode cap"), policy);
    wire["delay_limit"]["nanoseconds"] = json!(1_000_000_000);
    assert!(from_value::<BackoffPolicy>(wire).is_err());
}
