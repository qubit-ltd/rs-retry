// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::Retry;

/// Verifies a successful result exposes and consumes its value and context.
#[test]
fn test_retry_success_accessors_and_consumers() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("retry should build");

    let success = retry
        .run(|| Ok::<_, &'static str>("done"))
        .expect("operation should succeed");

    assert_eq!(&"done", success.value());
    assert_eq!(1, success.context().attempt());
    let (value, context) = success.into_parts();
    assert_eq!("done", value);
    assert_eq!(1, context.attempt());
}

/// Verifies a successful result can consume only its operation value.
#[test]
fn test_retry_success_into_value() {
    let retry = Retry::<()>::builder().build().expect("retry should build");

    let value = retry
        .run(|| Ok::<_, ()>(42))
        .expect("operation should succeed")
        .into_value();

    assert_eq!(42, value);
}
