// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::BackoffDelaySource;

#[test]
fn policy_is_a_stable_delay_source() {
    assert_eq!(BackoffDelaySource::Policy, BackoffDelaySource::Policy);
}
