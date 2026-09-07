use std::error::Error;
use std::fmt;

use crate::AttemptFailure;
use crate::AttemptFailureMetadata;
use crate::RetryCallbackFailure;
use crate::RetryContext;
use crate::RetryErrorMetadata;
use crate::RetryErrorReason;
use crate::RetrySuccess;

/// Error returned when a retry flow terminates without a successful result.
#[must_use]
#[derive(Debug)]
pub struct RetryError<E> {
    reason: RetryErrorReason,
    last_failure: Option<AttemptFailure<E>>,
    context: RetryContext,
    completion_callback_failures: Box<[RetryCallbackFailure]>,
}

/// Result alias returned by retry executor execution.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;

impl<E> RetryError<E> {
    pub(crate) fn new(
        reason: RetryErrorReason,
        last_failure: Option<AttemptFailure<E>>,
        context: RetryContext,
    ) -> Self {
        Self {
            reason,
            last_failure,
            context,
            completion_callback_failures: Box::new([]),
        }
    }

    #[must_use]
    pub const fn reason(&self) -> &RetryErrorReason {
        &self.reason
    }

    #[must_use]
    pub const fn context(&self) -> &RetryContext {
        &self.context
    }

    #[must_use]
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>> {
        self.last_failure.as_ref()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&E> {
        self.last_failure.as_ref().and_then(AttemptFailure::as_error)
    }

    #[must_use]
    pub fn completion_callback_failures(&self) -> &[RetryCallbackFailure] {
        &self.completion_callback_failures
    }

    pub fn map_error<U, F: FnOnce(E) -> U>(self, map: F) -> RetryError<U> {
        RetryError {
            reason: self.reason,
            last_failure: self.last_failure.map(|failure| failure.map_error(map)),
            context: self.context,
            completion_callback_failures: self.completion_callback_failures,
        }
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        RetryErrorReason,
        Option<AttemptFailure<E>>,
        RetryContext,
        Box<[RetryCallbackFailure]>,
    ) {
        (
            self.reason,
            self.last_failure,
            self.context,
            self.completion_callback_failures,
        )
    }

    #[must_use]
    pub fn into_metadata_and_error(self) -> (RetryErrorMetadata, Option<E>) {
        let (reason, last_failure, context, completion_callback_failures) = self.into_parts();
        let (last_attempt, application_error) = match last_failure {
            Some(AttemptFailure::Error(error)) => (Some(AttemptFailureMetadata::ApplicationError), Some(error)),
            Some(AttemptFailure::TimedOut { scope }) => (Some(AttemptFailureMetadata::TimedOut { scope }), None),
            Some(AttemptFailure::Panicked { panic }) => (Some(AttemptFailureMetadata::Panicked { panic }), None),
            None => (None, None),
        };
        (
            RetryErrorMetadata {
                reason,
                last_attempt,
                context,
                completion_callback_failures,
            },
            application_error,
        )
    }

    pub(crate) fn set_completion_callback_failures(&mut self, failures: Vec<RetryCallbackFailure>) {
        self.completion_callback_failures = failures.into_boxed_slice();
    }
}

impl<E: fmt::Display> fmt::Display for RetryError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "retry {} after {} attempt(s)",
            self.reason,
            self.context.attempts()
        )
    }
}

impl<E: Error + 'static> Error for RetryError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.last_error().map(|error| error as &(dyn Error + 'static))
    }
}
