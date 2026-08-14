// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryOutcomeKind;

#[test]
fn outcome_categories_are_distinct() {
    assert_ne!(RetryOutcomeKind::Succeeded, RetryOutcomeKind::Failed);
}
