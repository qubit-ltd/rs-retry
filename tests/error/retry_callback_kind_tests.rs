// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public callback-kind behavior.

use qubit_retry::RetryCallbackKind;

#[test]
fn test_retry_callback_kind_display() {
    let cases = [
        (RetryCallbackKind::Rule, "rule"),
        (RetryCallbackKind::Observer, "observer"),
    ];
    for (kind, expected) in cases {
        assert_eq!(kind.to_string(), expected);
    }
}
