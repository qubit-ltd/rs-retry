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
use qubit_retry::RetryPolicyBuilder;

use crate::support::FixedRetryRandomSource;

#[test]
fn test_policy_builders_cover_backoff_variants() {
    let deterministic = Arc::new(FixedRetryRandomSource::new(0.5));
    let mut immediate = BackoffPolicy::immediate().start();
    assert_eq!(BackoffPolicy::immediate().maximum_delay(), Some(Duration::ZERO));
    assert_eq!(
        immediate.next(BackoffRequest::policy()).effective_delay(),
        Duration::ZERO
    );

    let fixed = BackoffPolicy::fixed(Duration::from_millis(20));
    assert_eq!(fixed.maximum_delay(), Some(Duration::from_millis(20)));
    let mut fixed_state = fixed
        .clone()
        .without_jitter()
        .ignore_retry_after()
        .start_with_random_source(deterministic.clone());
    assert_eq!(
        fixed_state
            .next(BackoffRequest::hint(Duration::from_millis(50)))
            .effective_delay(),
        Duration::from_millis(20)
    );

    let uniform = BackoffPolicy::uniform(Duration::from_millis(10), Duration::from_millis(30))
        .unwrap()
        .with_full_jitter()
        .prefer_retry_after();
    assert_eq!(uniform.maximum_delay(), Some(Duration::from_millis(30)));
    let mut uniform_state = uniform.start_with_random_source(deterministic.clone());
    let hinted = uniform_state.next(BackoffRequest::jittered_hint(Duration::from_millis(15)));
    assert_eq!(hinted.retry_index(), 1);
    assert!(hinted.base_delay() <= Duration::from_millis(30));
    assert!(hinted.effective_delay() <= Duration::from_millis(15));
    let _ = hinted.source();

    let exponential = BackoffPolicy::exponential(Duration::from_millis(5), 2.0, Duration::from_millis(40))
        .unwrap()
        .with_bounded_jitter(0.25)
        .unwrap()
        .use_retry_after_as_minimum();
    assert_eq!(exponential.maximum_delay(), Some(Duration::from_millis(40)));
    let mut exponential_state = exponential.start_with_random_source(deterministic);
    assert!(
        exponential_state
            .next(BackoffRequest::hint(Duration::from_millis(12)))
            .effective_delay()
            >= Duration::from_millis(12)
    );

    for invalid in [f64::NAN, -0.1, 1.1] {
        let error = BackoffPolicy::immediate()
            .with_bounded_jitter(invalid)
            .expect_err("invalid jitter must fail");
        assert_eq!(error.field(), "backoff.jitter.ratio");
        assert!(!error.message().is_empty());
        assert!(error.to_string().contains("backoff.jitter.ratio"));
    }

    let policy = RetryPolicyBuilder::new()
        .max_attempts(4)
        .max_operation_elapsed(Duration::from_secs(1))
        .max_operation_elapsed_opt(Some(Duration::from_secs(2)))
        .without_operation_elapsed()
        .max_total_elapsed(Duration::from_secs(3))
        .max_total_elapsed_opt(Some(Duration::from_secs(4)))
        .without_total_elapsed()
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    assert_eq!(policy.limits().max_attempts().get(), 4);
    assert_eq!(
        RetryPolicyBuilder::default()
            .build()
            .unwrap()
            .limits()
            .max_attempts()
            .get(),
        3
    );

    let mut saturated = BackoffPolicy::exponential(Duration::MAX, f64::MAX, Duration::MAX)
        .unwrap()
        .with_bounded_jitter(1.0)
        .unwrap()
        .start_with_random_source(Arc::new(FixedRetryRandomSource::new(1.0)));
    let _ = saturated.next(BackoffRequest::policy());
    let _ = saturated.next(BackoffRequest::policy());
    let mut equal_uniform = BackoffPolicy::uniform(Duration::from_millis(4), Duration::from_millis(4))
        .unwrap()
        .with_full_jitter()
        .start_with_random_source(Arc::new(FixedRetryRandomSource::new(0.5)));
    let _ = equal_uniform.next(BackoffRequest::policy());
}
