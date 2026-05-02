/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Internal control flow after a failed attempt.

use std::time::Duration;

use crate::error::{AttemptFailure, RetryError};

/// Internal control flow after a failed attempt.
pub(in crate::executor) enum RetryFlowAction<E> {
    /// Retry after `delay`.
    Retry {
        /// Delay before the next attempt.
        delay: Duration,
        /// Failure from the attempt that just completed.
        failure: AttemptFailure<E>,
    },
    /// Finish with a terminal error.
    Finished(RetryError<E>),
}
