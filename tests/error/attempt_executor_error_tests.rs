/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::AttemptExecutorError;

/// Verifies executor failure messages are accessible and displayable.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_executor_error_message_and_display() {
    let error = AttemptExecutorError::new("worker spawn failed");

    assert_eq!(error.message(), "worker spawn failed");
    assert_eq!(error.to_string(), "worker spawn failed");
}
