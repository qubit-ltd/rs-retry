// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime work selected after one failed attempt.

use std::time::Duration;

/// Runtime work selected after one failed attempt.
pub(crate) struct RetryDirective {
    /// Sleep duration after applying the remaining hard-flow timeout.
    pub(super) sleep_duration: Duration,
}

impl RetryDirective {
    /// Returns the duration the executor should wait before retrying.
    pub(crate) fn sleep_duration(&self) -> Duration {
        self.sleep_duration
    }
}
