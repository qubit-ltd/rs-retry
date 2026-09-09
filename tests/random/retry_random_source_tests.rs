// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::RetryRandomSource;

use crate::support::FixedRetryRandomSource;

/// Verifies custom random sources can supply bounded floating-point samples.
#[test]
fn test_retry_random_source_exposes_float_samples() {
    let source = FixedRetryRandomSource::new(0.25);

    assert_eq!(source.random_f64_inclusive(0.0, 1.0), 0.25);
}

/// Exercises the default thread random source through public backoff sampling.
#[test]
fn test_default_random_source_keeps_uniform_jitter_in_bounds() {
    let mut state = BackoffPolicy::uniform(Duration::from_nanos(1), Duration::from_nanos(2))
        .expect("valid uniform range")
        .with_full_jitter()
        .start();
    assert!(state.next(BackoffRequest::policy()).effective_delay() <= Duration::from_nanos(2));
}
