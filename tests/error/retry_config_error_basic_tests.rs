/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::RetryConfigError;

/// Verifies basic configuration error accessors and empty-path formatting.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when accessor or formatting behavior changes.
#[test]
fn test_retry_config_error_accessors_and_empty_path_display() {
    let empty_path = RetryConfigError::invalid_value("", "missing value");
    assert_eq!(empty_path.path(), "");
    assert_eq!(empty_path.message(), "missing value");
    assert_eq!(
        empty_path.to_string(),
        "invalid retry configuration: missing value"
    );

    let keyed = RetryConfigError::invalid_value("retry.max_attempts", "must be positive");
    assert_eq!(keyed.path(), "retry.max_attempts");
    assert_eq!(keyed.message(), "must be positive");
    assert_eq!(
        keyed.to_string(),
        "invalid retry configuration at 'retry.max_attempts': must be positive"
    );
}
