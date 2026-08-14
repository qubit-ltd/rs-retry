// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[cfg(feature = "tokio")]
use qubit_retry::Retry;
#[cfg(feature = "tokio")]
use qubit_retry::RetryPolicy;

#[cfg(feature = "tokio")]
#[test]
fn async_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.asynchronous();
}
