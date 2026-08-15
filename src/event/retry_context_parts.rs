// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal retry context constructor payload.

use std::num::NonZeroU32;
use std::time::Duration;

/// Internal context constructor payload.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryContextParts {
    /// Number of operations that actually started.
    pub(crate) attempts: u32,
    /// Current callback or operation attempt ordinal.
    pub(crate) current_attempt: Option<NonZeroU32>,
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
    /// Elapsed time spent in the last completed attempt.
    pub(crate) last_attempt_elapsed: Duration,
    /// Effective timeout configured for the current attempt.
    pub(crate) current_attempt_timeout: Option<Duration>,
    /// Delay selected before the next attempt, when known.
    pub(crate) next_delay: Option<Duration>,
    /// Optional retry-after hint extracted before failure policy runs.
    pub(crate) retry_after_hint: Option<Duration>,
}
