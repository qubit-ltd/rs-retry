/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetryAbortContext;

/// Verifies abort context carries expected terminal metadata fields.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when abort context fields mismatch.
#[test]
fn test_abort_context_fields() {
    let context = RetryAbortContext {
        attempts: 1,
        elapsed: Duration::from_millis(7),
    };
    assert_eq!(context.attempts, 1);
    assert_eq!(context.elapsed, Duration::from_millis(7));
}
