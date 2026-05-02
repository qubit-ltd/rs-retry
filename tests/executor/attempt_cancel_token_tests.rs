/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

use qubit_retry::AttemptCancelToken;

/// Verifies a new cancellation token starts in the non-cancelled state.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_cancel_token_new_starts_not_cancelled() {
    let token = AttemptCancelToken::new();

    assert!(!token.is_cancelled());
}

/// Verifies cancellation is visible through cloned tokens.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_attempt_cancel_token_cancel_is_shared_by_clones() {
    let token = AttemptCancelToken::new();
    let clone = token.clone();

    token.cancel();

    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
}
