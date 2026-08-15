// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public worker-stop-trigger behavior.

use qubit_retry::WorkerStopTrigger;

#[test]
fn test_worker_stop_trigger_display() {
    let cases = [
        (WorkerStopTrigger::AttemptTimeout, "attempt timeout"),
        (WorkerStopTrigger::FlowTimeout, "flow timeout"),
        (WorkerStopTrigger::Cancellation, "cancellation"),
    ];
    for (trigger, expected) in cases {
        assert_eq!(trigger.to_string(), expected);
    }
}
