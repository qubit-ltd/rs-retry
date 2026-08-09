// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptFailureKind;

/// Verifies attempt failure kinds are stable, comparable, and serializable.
#[test]
fn test_attempt_failure_kind_variants_are_observable() {
    let kinds = [
        AttemptFailureKind::Application,
        AttemptFailureKind::TimedOut,
        AttemptFailureKind::Panicked,
        AttemptFailureKind::Infrastructure,
    ];

    assert_eq!(kinds[0], AttemptFailureKind::Application);
    assert_eq!(kinds[1], AttemptFailureKind::TimedOut);
    assert_eq!(kinds[2], AttemptFailureKind::Panicked);
    assert_eq!(kinds[3], AttemptFailureKind::Infrastructure);
    assert_eq!(
        serde_json::to_string(&AttemptFailureKind::TimedOut).unwrap(),
        "\"TimedOut\""
    );
}
