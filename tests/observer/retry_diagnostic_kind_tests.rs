// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryDiagnosticKind;

#[test]
fn observer_is_a_stable_diagnostic_kind() {
    assert_eq!(
        RetryDiagnosticKind::ObserverPanicked,
        RetryDiagnosticKind::ObserverPanicked
    );
}
