// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public infrastructure-failure behavior.

use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::WorkerStopTrigger;

#[test]
fn test_retry_infrastructure_failure_accessors_and_display() {
    let cases = [
        (
            RetryInfrastructureFailure::Clock {
                message: "clock unavailable".into(),
            },
            Some("clock unavailable"),
            None,
            "clock failed: clock unavailable",
        ),
        (
            RetryInfrastructureFailure::Timer {
                message: "timer unavailable".into(),
            },
            Some("timer unavailable"),
            None,
            "timer failed: timer unavailable",
        ),
        (
            RetryInfrastructureFailure::WorkerSpawn {
                message: "worker unavailable".into(),
            },
            Some("worker unavailable"),
            None,
            "worker spawn failed: worker unavailable",
        ),
        (
            RetryInfrastructureFailure::WorkerStillRunning {
                trigger: WorkerStopTrigger::FlowTimeout,
            },
            None,
            Some(WorkerStopTrigger::FlowTimeout),
            "worker still running after flow timeout",
        ),
    ];
    for (failure, message, trigger, expected) in cases {
        assert_eq!(failure.message(), message);
        assert_eq!(failure.worker_stop_trigger(), trigger);
        assert_eq!(failure.to_string(), expected);
    }
}
