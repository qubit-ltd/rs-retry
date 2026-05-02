/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Internal retry context constructor payload.

use std::time::Duration;

/// Internal context constructor payload.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryContextParts {
    /// Current attempt number, or zero if no attempt has run.
    pub(crate) attempt: u32,
    /// Configured maximum attempts.
    pub(crate) max_attempts: u32,
    /// Configured maximum cumulative user operation time.
    pub(crate) max_operation_elapsed: Option<Duration>,
    /// Configured maximum total retry-flow elapsed time.
    pub(crate) max_total_elapsed: Option<Duration>,
    /// Cumulative user operation time consumed by this retry flow.
    pub(crate) operation_elapsed: Duration,
    /// Total monotonic time consumed by this retry flow.
    pub(crate) total_elapsed: Duration,
    /// Elapsed time spent in the current attempt.
    pub(crate) attempt_elapsed: Duration,
    /// Effective timeout configured for the current attempt.
    pub(crate) attempt_timeout: Option<Duration>,
}
