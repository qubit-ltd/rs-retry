// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Non-generic metadata retained from a terminal retry error.

use std::error::Error;
use std::fmt;

use crate::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorReason;

/// Non-generic terminal metadata that can be retained by downstream errors.
///
/// # Examples
///
/// Metadata can be obtained by consuming a public terminal retry error:
///
/// ```
/// use qubit_retry::AttemptFailureMetadata;
/// use qubit_retry::Retry;
/// use qubit_retry::RetryConfig;
///
/// let config = RetryConfig::<&str>::builder().build()?;
/// let error = Retry::new(&config).run(|| Err::<(), _>("offline"))
///     .expect_err("unclassified failure aborts");
/// let (metadata, application_error) = error.into_metadata_and_error();
/// assert_eq!(application_error, Some("offline"));
/// assert_eq!(metadata.context().attempts(), 1);
/// assert!(matches!(metadata.last_attempt(), Some(AttemptFailureMetadata::ApplicationError)));
/// assert!(metadata.completion_callback_failures().is_empty());
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[derive(Debug)]
pub struct RetryErrorMetadata {
    /// Terminal reason independent of the application error type.
    pub(crate) reason: RetryErrorReason,
    /// Classification of the retained last failure, when available.
    pub(crate) last_attempt: Option<AttemptFailureMetadata>,
    /// Frozen retry context at termination.
    pub(crate) context: RetryContext,
    /// Completion callback diagnostics captured after termination.
    pub(crate) completion_callback_failures: Box<[RetryCallbackFailure]>,
}

impl RetryErrorMetadata {
    /// Returns the terminal reason.
    ///
    /// # Returns
    /// The reason that ended the retry flow.
    #[must_use = "inspect the terminal reason"]
    #[inline]
    pub const fn reason(&self) -> &RetryErrorReason {
        &self.reason
    }

    /// Returns the classification of the retained last failure.
    ///
    /// This is not a record of whether the latest operation started.
    /// Cancellation or an infrastructure failure may terminate an admitted
    /// operation without recording an attempt failure; an earlier failure may
    /// remain retained. Inspect [`context`](Self::context) and [`reason`](Self::reason)
    /// together when diagnosing execution.
    ///
    /// # Returns
    /// `Some` with the retained failure classification, or `None` when no
    /// failure was retained, even if one or more attempts were admitted.
    #[inline]
    pub const fn last_attempt(&self) -> Option<&AttemptFailureMetadata> {
        self.last_attempt.as_ref()
    }

    /// Returns the frozen retry context.
    ///
    /// # Returns
    /// The retry context captured when the flow terminated.
    #[must_use = "inspect the terminal context"]
    #[inline]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns completion callback failures captured for this result.
    ///
    /// # Returns
    /// The callback failures in registration order; an empty slice means no
    /// completion callback panicked.
    #[must_use = "inspect completion callback diagnostics"]
    #[inline]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }
}

impl fmt::Display for RetryErrorMetadata {
    /// Formats the terminal reason and number of admitted attempts.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "retry {} after {} attempt(s)",
            self.reason,
            self.context.attempts()
        )
    }
}

impl Error for RetryErrorMetadata {}
