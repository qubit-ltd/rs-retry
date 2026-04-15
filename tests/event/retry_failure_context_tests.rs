/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetryFailureContext;

/// Verifies failure context carries expected terminal metadata fields.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when failure context fields mismatch.
#[test]
fn test_failure_context_fields() {
    let context = RetryFailureContext {
        attempts: 3,
        elapsed: Duration::from_millis(11),
    };
    assert_eq!(context.attempts, 3);
    assert_eq!(context.elapsed, Duration::from_millis(11));
}
