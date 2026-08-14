// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptCancellationToken;

/// Verifies a new cancellation token starts in the non-cancelled state.
#[test]
fn test_attempt_cancellation_token_new_starts_not_cancelled() {
    let token = AttemptCancellationToken::new();

    assert!(!token.is_cancelled());
}

/// Verifies cancellation is visible through cloned tokens.
#[test]
fn test_attempt_cancellation_token_cancel_is_shared_by_clones() {
    let token = AttemptCancellationToken::new();
    let clone = token.clone();

    token.cancel();

    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
}
