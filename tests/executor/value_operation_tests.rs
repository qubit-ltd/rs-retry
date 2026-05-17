/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::Retry;

/// Verifies sync value capture supports non-clone success values.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_value_operation_is_observable_through_non_clone_success_value() {
    #[derive(Debug, PartialEq, Eq)]
    struct Token(String);

    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .unwrap();

    let value = retry.run(|| Ok(Token("captured".to_owned()))).unwrap();
    assert_eq!(Token("captured".to_owned()), value);
}
