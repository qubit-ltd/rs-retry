// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_retry::RetryObserver;

struct NoopObserver;

impl RetryObserver<()> for NoopObserver {}

#[test]
fn observer_trait_accepts_function_callbacks() {
    let observer: Box<dyn RetryObserver<()>> = Box::new(NoopObserver);
    let _ = observer;
}
