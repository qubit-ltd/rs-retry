/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Effective timeout selected for a single retry attempt.

use std::time::Duration;

use crate::{
    AttemptFailure,
    AttemptTimeoutSource,
    RetryErrorReason,
};

/// Effective timeout selected for a single attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::executor) struct EffectiveAttemptTimeout {
    /// Timeout duration actually enforced for the attempt.
    pub(in crate::executor) duration: Option<Duration>,
    /// Source that selected the effective timeout.
    pub(in crate::executor) source: Option<AttemptTimeoutSource>,
}

impl EffectiveAttemptTimeout {
    /// Creates an effective attempt timeout.
    ///
    /// # Parameters
    /// - `duration`: Timeout duration enforced for the attempt.
    /// - `source`: Source that selected the timeout.
    ///
    /// # Returns
    /// A timeout descriptor for one attempt.
    #[inline]
    pub(in crate::executor) fn new(
        duration: Option<Duration>,
        source: Option<AttemptTimeoutSource>,
    ) -> Self {
        Self { duration, source }
    }

    /// Returns the elapsed-budget reason represented by a timeout failure.
    ///
    /// # Parameters
    /// - `failure`: Failure produced by the attempt.
    ///
    /// # Returns
    /// `Some(RetryErrorReason)` when the attempt timed out because an elapsed
    /// budget selected the effective timeout.
    #[inline]
    pub(in crate::executor) fn elapsed_timeout_reason<E>(
        &self,
        failure: &AttemptFailure<E>,
    ) -> Option<RetryErrorReason> {
        if !matches!(failure, AttemptFailure::Timeout) {
            return None;
        }
        match self.source {
            Some(AttemptTimeoutSource::MaxOperationElapsed) => {
                Some(RetryErrorReason::MaxOperationElapsedExceeded)
            }
            Some(AttemptTimeoutSource::MaxTotalElapsed) => {
                Some(RetryErrorReason::MaxTotalElapsedExceeded)
            }
            Some(AttemptTimeoutSource::Configured) | None => None,
        }
    }
}
