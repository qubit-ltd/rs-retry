// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryFallback;
use qubit_retry::RetryPolicy;

use crate::support::UnitTestError;

#[test]
fn test_sync_facade_retries_application_failure() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let retry = RetryConfig::<UnitTestError>::builder()
        .policy(policy)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("valid config");
    let attempts = AtomicU32::new(0);
    let result = Retry::new(&retry).run(|| {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(UnitTestError)
        } else {
            Ok(42_u32)
        }
    });
    let success = result.expect("second attempt succeeds");
    assert_eq!(*success.value(), 42);
    assert_eq!(success.context().attempts(), 2);
}
