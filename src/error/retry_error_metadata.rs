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

use super::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorReason;

/// Non-generic terminal metadata that can be retained by downstream errors.
#[derive(Debug)]
pub struct RetryErrorMetadata {
    /// Terminal reason independent of the application error type.
    pub(crate) reason: RetryErrorReason,
    /// Classification of the last attempt, when an attempt was started.
    pub(crate) last_attempt: Option<AttemptFailureMetadata>,
    /// Frozen retry context at termination.
    pub(crate) context: RetryContext,
    /// Completion callback diagnostics captured after termination.
    pub(crate) completion_callback_failures: Box<[RetryCallbackFailure]>,
}

impl RetryErrorMetadata {
    /// Returns the terminal reason.
    #[must_use]
    pub const fn reason(&self) -> &RetryErrorReason {
        &self.reason
    }

    /// Returns the classification of the last attempt, when present.
    #[must_use]
    pub const fn last_attempt(&self) -> Option<&AttemptFailureMetadata> {
        self.last_attempt.as_ref()
    }

    /// Returns the frozen retry context.
    #[must_use]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Returns completion callback failures captured for this result.
    #[must_use]
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
