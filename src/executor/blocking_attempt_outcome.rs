/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Result and cleanup status returned from one blocking worker attempt.

use crate::AttemptFailure;

/// Result and cleanup status returned from one blocking worker attempt.
pub(in crate::executor) struct BlockingAttemptOutcome<T, E> {
    /// Attempt result after timeout handling.
    pub(in crate::executor) result: Result<T, AttemptFailure<E>>,
    /// Worker threads not observed to exit before cancellation grace ended.
    pub(in crate::executor) unreaped_worker_count: u32,
}

impl<T, E> BlockingAttemptOutcome<T, E> {
    /// Creates a worker-attempt outcome.
    ///
    /// # Parameters
    /// - `result`: Attempt result exposed to the retry flow.
    /// - `unreaped_worker_count`: Count of worker threads not observed to exit.
    ///
    /// # Returns
    /// A blocking-attempt outcome.
    #[inline]
    pub(in crate::executor) fn new(
        result: Result<T, AttemptFailure<E>>,
        unreaped_worker_count: u32,
    ) -> Self {
        Self {
            result,
            unreaped_worker_count,
        }
    }
}
