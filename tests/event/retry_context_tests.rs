// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::RetryContext;

/// Verifies a context before any operation has no current attempt.
#[test]
fn test_retry_context_before_first_attempt() {
    let context = RetryContext::new(0, 5);
    assert_eq!(context.attempts(), 0);
    assert_eq!(context.current_attempt(), None);
}

/// Verifies the first started operation is counted and remains current.
#[test]
fn test_retry_context_first_operation_started() {
    let context = RetryContext::new(1, 5);
    assert_eq!(context.attempts(), 1);
    assert_eq!(
        context.current_attempt().map(std::num::NonZeroU32::get),
        Some(1)
    );
}

/// Verifies retry context carries the renamed timing metadata fields.
#[test]
fn test_retry_context_fields() {
    let context = RetryContext::new(2, 5);
    assert_eq!(context.attempts(), 2);
    assert_eq!(
        context.current_attempt().map(std::num::NonZeroU32::get),
        Some(2)
    );
    assert_eq!(context.max_attempts(), 5);
    assert_eq!(context.max_retries(), 4);
    assert_eq!(context.max_operation_elapsed(), None);
    assert_eq!(context.max_total_elapsed(), None);
    assert_eq!(context.operation_elapsed(), Duration::ZERO);
    assert_eq!(context.total_elapsed(), Duration::ZERO);
    assert_eq!(context.last_attempt_elapsed(), Duration::ZERO);
    assert_eq!(context.current_attempt_timeout(), None);
    assert_eq!(context.next_delay(), None);
    assert_eq!(context.retry_after_hint(), None);
}
