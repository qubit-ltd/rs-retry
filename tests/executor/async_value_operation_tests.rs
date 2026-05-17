/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

#[cfg(feature = "tokio")]
use qubit_retry::Retry;

/// Verifies async value capture is observable through `Retry::async_run`.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_value_operation_is_observable_through_async_success_value() {
    #[derive(Debug, PartialEq, Eq)]
    struct Token(String);

    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .unwrap();

    let value = retry
        .async_run(|| async { Ok(Token("captured".to_owned())) })
        .await
        .unwrap();
    assert_eq!(Token("captured".to_owned()), value);
}
