/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetryContext;

/// Verifies retry context carries expected retry metadata fields.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when retry context fields mismatch.
#[test]
fn test_retry_context_fields() {
    let context = RetryContext {
        attempt: 2,
        max_attempts: 5,
        elapsed: Duration::from_millis(8),
        next_delay: Duration::from_millis(3),
    };
    assert_eq!(context.attempt, 2);
    assert_eq!(context.max_attempts, 5);
    assert_eq!(context.elapsed, Duration::from_millis(8));
    assert_eq!(context.next_delay, Duration::from_millis(3));
}
