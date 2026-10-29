// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::AttemptCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::WorkerRetry;

fn main() {
    let _ = AttemptCancellationToken::new();
    let config = RetryConfig::<()>::builder()
        .build()
        .expect("valid fixture config");
    let _ = WorkerRetry::new(&config);
}
