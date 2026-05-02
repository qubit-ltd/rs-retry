/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::AttemptPanic;

/// Verifies captured panic messages are accessible and displayable.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_panic_message_and_display() {
    let panic = AttemptPanic::new("worker failed");

    assert_eq!(panic.message(), "worker failed");
    assert_eq!(panic.to_string(), "worker failed");
}
