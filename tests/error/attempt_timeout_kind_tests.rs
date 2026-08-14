// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptTimeoutKind;

#[test]
fn exposes_timeout_boundaries() {
    assert_ne!(AttemptTimeoutKind::Attempt, AttemptTimeoutKind::Flow);
}
