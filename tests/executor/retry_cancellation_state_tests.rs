// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public cancellation-state behavior.

use qubit_retry::RetryCancellationToken;

#[test]
fn test_retry_cancellation_state_is_shared_by_public_token_clones() {
    let token = RetryCancellationToken::new();
    let clone = token.clone();
    clone.cancel();

    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
}
