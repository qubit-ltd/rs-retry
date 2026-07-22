// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryRandomSource;

use crate::support::FixedRetryRandomSource;

/// Verifies custom random sources can supply both supported sample types.
#[test]
fn test_retry_random_source_exposes_integer_and_float_samples() {
    let source = FixedRetryRandomSource::new(7, 0.25);

    assert_eq!(source.random_u64_inclusive(5, 9), 7);
    assert_eq!(source.random_f64_inclusive(0.0, 1.0), 0.25);
}
