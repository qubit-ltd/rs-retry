// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryPolicy;

#[test]
fn rejects_zero_attempts() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
}
