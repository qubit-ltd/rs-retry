// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::{
    RetryDelay,
    RetryJitter,
};

/// Verifies default random delay sampling remains within its inclusive bounds.
#[test]
fn test_thread_retry_random_source_samples_delay_within_bounds() {
    let delay =
        RetryDelay::random(Duration::from_nanos(5), Duration::from_nanos(9));

    for _ in 0..32 {
        let sample = delay.base_delay(1);
        assert!(sample >= Duration::from_nanos(5));
        assert!(sample <= Duration::from_nanos(9));
    }
}

/// Verifies default floating-point jitter sampling remains within its bounds.
#[test]
fn test_thread_retry_random_source_samples_jitter_within_bounds() {
    let base = Duration::from_nanos(100);
    let jitter = RetryJitter::factor(0.25);

    for _ in 0..32 {
        let sample = jitter.apply(base);
        assert!(sample >= Duration::from_nanos(75));
        assert!(sample <= Duration::from_nanos(125));
    }
}
