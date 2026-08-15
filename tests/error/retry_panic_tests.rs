// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public panic-payload behavior.

use qubit_retry::RetryPanic;

#[test]
fn test_retry_panic_accessors_and_display() {
    let static_text = RetryPanic::StaticStr("static panic");
    let owned_text = RetryPanic::String("owned panic".to_owned());
    let non_string = RetryPanic::NonString;
    assert_eq!(static_text.message(), Some("static panic"));
    assert_eq!(static_text.to_string(), "static panic");
    assert_eq!(owned_text.message(), Some("owned panic"));
    assert_eq!(owned_text.to_string(), "owned panic");
    assert_eq!(non_string.message(), None);
    assert_eq!(non_string.to_string(), "non-string panic payload");
}
