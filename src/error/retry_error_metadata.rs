use std::error::Error;
use std::fmt;

use super::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorReason;

/// Non-generic terminal metadata that can be retained by downstream errors.
#[derive(Debug)]
pub struct RetryErrorMetadata {
    pub(crate) reason: RetryErrorReason,
    pub(crate) last_attempt: Option<AttemptFailureMetadata>,
    pub(crate) context: RetryContext,
    pub(crate) completion_callback_failures: Box<[RetryCallbackFailure]>,
}

impl RetryErrorMetadata {
    #[must_use]
    pub const fn reason(&self) -> &RetryErrorReason {
        &self.reason
    }

    #[must_use]
    pub const fn last_attempt(&self) -> Option<&AttemptFailureMetadata> {
        self.last_attempt.as_ref()
    }

    #[must_use]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    #[must_use]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }
}

impl fmt::Display for RetryErrorMetadata {
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
