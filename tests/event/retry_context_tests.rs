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
    let context = RetryContext::new(2, 5);
    assert_eq!(context.attempt(), 2);
    assert_eq!(context.max_attempts(), 5);
    assert_eq!(context.max_retries(), 4);
    assert_eq!(context.max_operation_elapsed(), None);
    assert_eq!(context.max_total_elapsed(), None);
    assert_eq!(context.operation_elapsed(), Duration::ZERO);
    assert_eq!(context.total_elapsed(), Duration::ZERO);
    assert_eq!(context.attempt_elapsed(), Duration::ZERO);
    assert_eq!(context.attempt_timeout(), None);
    assert_eq!(context.unreaped_worker_count(), 0);
    assert_eq!(context.next_delay(), None);
    assert_eq!(context.retry_after_hint(), None);
}
