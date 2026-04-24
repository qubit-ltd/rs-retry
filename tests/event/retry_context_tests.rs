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
    let context = RetryContext::new(
        2,
        5,
        Some(Duration::from_secs(1)),
        Duration::from_millis(8),
        Duration::from_millis(4),
        Some(Duration::from_millis(10)),
    );
    assert_eq!(context.attempt(), 2);
    assert_eq!(context.max_attempts(), 5);
    assert_eq!(context.max_retries(), 4);
    assert_eq!(context.max_elapsed(), Some(Duration::from_secs(1)));
    assert_eq!(context.total_elapsed(), Duration::from_millis(8));
    assert_eq!(context.attempt_elapsed(), Duration::from_millis(4));
    assert_eq!(context.attempt_timeout(), Some(Duration::from_millis(10)));
    assert_eq!(context.next_delay(), None);
    assert_eq!(context.retry_after_hint(), None);
}
