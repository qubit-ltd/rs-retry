// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryDiagnosticKind;

#[test]
fn exposes_callback_diagnostic_categories() {
    assert_ne!(
        RetryDiagnosticKind::RulePanicked,
        RetryDiagnosticKind::ObserverPanicked
    );
}
