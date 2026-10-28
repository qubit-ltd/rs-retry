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

use crate::support::FixedRetryRandomSource;

#[test]
fn test_backoff_delay_source_combinations() {
    let base = BackoffPolicy::fixed(Duration::from_millis(100)).with_full_jitter();
    let hint = Duration::from_millis(180);
    let cases = [
        (base.clone(), BackoffRequest::policy(), 50, BackoffDelaySource::Policy),
        (
            base.clone(),
            BackoffRequest::hint(hint),
            180,
            BackoffDelaySource::Merged,
        ),
        (
            base.clone(),
            BackoffRequest::jittered_hint(hint),
            90,
            BackoffDelaySource::Merged,
        ),
        (
            base.clone().prefer_retry_after(),
            BackoffRequest::jittered_hint(hint),
            90,
            BackoffDelaySource::Hint,
        ),
        (
            base.ignore_retry_after(),
            BackoffRequest::hint(hint),
            50,
            BackoffDelaySource::Policy,
        ),
    ];
    for (policy, request, delay_ms, source) in cases {
        let mut state = policy.start_with_random_source(Arc::new(FixedRetryRandomSource::new(0.5)));
        let step = state.next(request);
        assert_eq!(step.retry_index(), 1);
        assert_eq!(step.base_delay(), Duration::from_millis(100));
        assert_eq!(step.effective_delay(), Duration::from_millis(delay_ms));
        assert_eq!(step.source(), source);
    }
}
