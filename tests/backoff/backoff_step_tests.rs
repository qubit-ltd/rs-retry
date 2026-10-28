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
fn test_backoff_step_cap_preserves_base_and_ordinal() {
    let policy = BackoffPolicy::fixed(Duration::from_millis(100))
        .with_full_jitter()
        .limit_delay(Duration::from_millis(40));
    let mut state = policy.start_with_random_source(Arc::new(FixedRetryRandomSource::new(0.5)));
    for expected_index in [1, 2] {
        let step = state.next(BackoffRequest::hint(Duration::from_millis(180)));
        assert_eq!(step.retry_index(), expected_index);
        assert_eq!(step.base_delay(), Duration::from_millis(100));
        assert_eq!(step.effective_delay(), Duration::from_millis(40));
        assert_eq!(step.source(), BackoffDelaySource::Merged);
    }
}
