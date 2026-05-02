/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Attempt failure decisions.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Decision returned by a retry failure listener after inspecting a failure.
///
/// Explicit retry decisions still obey attempt and cumulative user operation
/// elapsed-time limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptFailureDecision {
    /// Use the retry policy's default decision for this failure.
    UseDefault,
    /// Retry the operation if limits still allow it.
    Retry,
    /// Retry after the specified delay if limits still allow it.
    RetryAfter(
        /// Delay selected by the listener.
        #[serde(with = "qubit_serde::serde::duration_millis")]
        Duration,
    ),
    /// Abort immediately and return the current failure.
    Abort,
}

impl Default for AttemptFailureDecision {
    /// Returns the default decision.
    ///
    /// # Returns
    /// [`AttemptFailureDecision::UseDefault`].
    #[inline]
    fn default() -> Self {
        Self::UseDefault
    }
}
