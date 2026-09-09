// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailure;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryRule;

#[test]
fn test_rule_trait_accepts_function_callbacks() {
    let rule: Box<dyn RetryRule<()>> =
        Box::new(|failure: &AttemptFailure<()>, context: &RetryContext| {
            assert_eq!(failure, &AttemptFailure::Error(()));
            assert_eq!(context.attempts(), 0);
            RetryDecision::Abort
        });
    assert_eq!(
        rule.decide(&AttemptFailure::Error(()), &RetryContext::new(0, 1)),
        RetryDecision::Abort
    );
}
