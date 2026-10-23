// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests for retry continuation exhaustion facts.

use qubit_retry::RetryBudgetExhausted;

/// Verifies continuation exhaustion categories remain distinct values.
#[test]
fn test_categories_are_distinct() {
    assert_ne!(
        RetryBudgetExhausted::Attempts,
        RetryBudgetExhausted::OperationElapsed,
        "attempt exhaustion must remain distinct from operation elapsed exhaustion"
    );
    assert_ne!(
        RetryBudgetExhausted::OperationElapsed,
        RetryBudgetExhausted::TotalElapsed,
        "operation elapsed exhaustion must remain distinct from total elapsed exhaustion"
    );
}
