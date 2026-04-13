/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Internal failed-attempt control flow.
//!
//! `FailureAction` is the private return type used by the retry executor after a
//! failed attempt has been classified and checked against retry limits.

use std::time::Duration;

use crate::{AttemptFailure, RetryError};

/// Action selected after handling one failed attempt.
///
/// The generic parameter `E` is the caller's application error type preserved
/// inside attempt failures and terminal retry errors.
pub(super) enum FailureAction<E> {
    /// Continue retrying after sleeping for the computed delay.
    Retry {
        /// Delay to sleep before running the next attempt.
        delay: Duration,
        /// Failure from the attempt that just completed.
        failure: AttemptFailure<E>,
    },
    /// Stop retrying and return the terminal retry error.
    Finished(RetryError<E>),
}
