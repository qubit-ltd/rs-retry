/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::AttemptFailureDecision;

/// Verifies the default failure decision delegates to the retry policy.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when the default decision changes.
#[test]
fn test_attempt_failure_decision_default_uses_policy_default() {
    assert_eq!(
        AttemptFailureDecision::default(),
        AttemptFailureDecision::UseDefault
    );
}
