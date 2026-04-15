/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetrySuccessContext;

/// Verifies success context carries expected execution metadata fields.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when success context fields mismatch.
#[test]
fn test_success_context_fields() {
    let context = RetrySuccessContext {
        attempts: 2,
        elapsed: Duration::from_millis(8),
    };
    assert_eq!(context.attempts, 2);
    assert_eq!(context.elapsed, Duration::from_millis(8));
}
