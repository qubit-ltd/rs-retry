// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryContext;
use qubit_retry::RetryObserver;

use super::TestError;

pub(crate) struct AdvancingObserver(pub(crate) Arc<ManualMonotonicClock>);

impl RetryObserver<TestError> for AdvancingObserver {
    fn on_before_attempt(&self, _context: &RetryContext) {
        self.0.advance(Duration::from_secs(2)).unwrap();
    }
}
