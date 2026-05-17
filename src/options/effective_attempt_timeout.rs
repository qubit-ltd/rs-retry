/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Effective timeout selected from retry options for a single attempt.
//!
//! Executors need both the duration to enforce and the reason that duration was
//! selected. A fired configured attempt timeout can be retried or aborted by
//! policy, while a fired elapsed-budget timeout is already the terminal reason
//! for the whole retry flow.

use std::time::Duration;

use crate::{
    AttemptFailure,
    AttemptTimeoutSource,
    RetryErrorReason,
};

/// Effective timeout selected for a single attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EffectiveAttemptTimeout {
    /// Timeout duration actually enforced for the attempt.
    duration: Option<Duration>,
    /// Source that selected the effective timeout.
    source: Option<AttemptTimeoutSource>,
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
    pub(crate) fn new(duration: Option<Duration>, source: Option<AttemptTimeoutSource>) -> Self {
        Self { duration, source }
    }

    /// Creates an unbounded attempt timeout.
    ///
    /// # Returns
    /// A timeout descriptor with no duration and no source.
    #[inline]
    pub(crate) fn none() -> Self {
        Self::new(None, None)
    }

    /// Returns the timeout duration.
    ///
    /// # Returns
    /// `Some(Duration)` when this attempt is bounded.
    #[inline]
    pub(crate) fn duration(self) -> Option<Duration> {
        self.duration
    }

    /// Returns the timeout source.
    ///
    /// # Returns
    /// `Some(AttemptTimeoutSource)` when this timeout came from a configured or
    /// elapsed-budget limit.
    #[inline]
    pub(crate) fn source(self) -> Option<AttemptTimeoutSource> {
        self.source
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
    pub(crate) fn elapsed_timeout_reason<E>(
        self,
        failure: &AttemptFailure<E>,
    ) -> Option<RetryErrorReason> {
        if !matches!(failure, AttemptFailure::Timeout) {
            return None;
        }
        // Only elapsed-budget timeout sources are terminal here. Configured
        // timeouts intentionally return None so the caller routes them through
        // RetryFailureHandler and AttemptTimeoutPolicy.
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
