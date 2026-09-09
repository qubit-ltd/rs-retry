// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::time::Duration;

use qubit_retry::BackoffDelaySource;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::RetryRandomSource;

use crate::support::FixedRetryRandomSource;

struct FixedRandom;

impl RetryRandomSource for FixedRandom {
    fn random_f64_inclusive(&self, min: f64, _max: f64) -> f64 {
        min
    }
}

#[test]
fn test_backoff_state_advances_and_resets() {
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(10),
        2.0,
        Duration::from_millis(25),
    )
    .expect("valid exponential policy");
    let mut state = policy.start_with_random_source(Arc::new(FixedRandom));
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(10)
    );
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(20)
    );
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(25)
    );
    state.reset();
    assert_eq!(state.retry_index(), 0);
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
}

#[test]
fn test_retry_after_hint_obeys_policy() {
    let policy = BackoffPolicy::fixed(Duration::from_secs(1));
    let mut state = policy.start_with_random_source(Arc::new(FixedRandom));
    let step = state.next(BackoffRequest::hint(Duration::from_secs(3)));
    assert_eq!(step.effective_delay(), Duration::from_secs(3));
    assert_eq!(step.source(), BackoffDelaySource::Merged);
}

#[test]
fn test_retry_after_hint_modes_select_expected_delay_source() {
    let hint = Duration::from_secs(3);
    let prefer =
        BackoffPolicy::fixed(Duration::from_secs(1)).prefer_retry_after();
    let mut prefer_state = prefer.start();
    let prefer_step = prefer_state.next(BackoffRequest::hint(hint));
    assert_eq!(prefer_step.effective_delay(), hint);
    assert_eq!(prefer_step.source(), BackoffDelaySource::Hint);

    let ignore =
        BackoffPolicy::fixed(Duration::from_secs(1)).ignore_retry_after();
    let mut ignore_state = ignore.start();
    let ignore_step = ignore_state.next(BackoffRequest::hint(hint));
    assert_eq!(ignore_step.effective_delay(), Duration::from_secs(1));
    assert_eq!(ignore_step.source(), BackoffDelaySource::Policy);

    let jittered =
        BackoffPolicy::fixed(Duration::from_secs(1)).with_full_jitter();
    let mut jittered_state = jittered
        .start_with_random_source(Arc::new(FixedRetryRandomSource::new(0.5)));
    let jittered_step =
        jittered_state.next(BackoffRequest::jittered_hint(hint));
    assert_eq!(jittered_step.effective_delay(), Duration::from_millis(1500));
    assert_eq!(jittered_step.source(), BackoffDelaySource::Merged);
}
