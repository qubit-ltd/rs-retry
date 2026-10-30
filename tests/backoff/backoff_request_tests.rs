// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::time::Duration;

use qubit_retry::BackoffRequest;

#[test]
fn test_constructors_capture_public_delay_inputs() {
    let delay = Duration::from_millis(1);
    let policy_request = BackoffRequest::policy();
    let hinted_request = BackoffRequest::hint(delay);
    let jittered_hint_request = BackoffRequest::jittered_hint(delay);

    assert_ne!(
        policy_request, hinted_request,
        "policy and hinted requests must represent different inputs"
    );
    assert_ne!(
        hinted_request, jittered_hint_request,
        "hinted and jittered-hint requests must represent different inputs"
    );
}
