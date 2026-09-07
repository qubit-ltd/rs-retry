use super::RetryPanic;
use super::RetryTimeoutScope;

/// Attempt classification retained after the application error is moved out.
#[derive(Debug)]
#[non_exhaustive]
pub enum AttemptFailureMetadata {
    /// The attempt returned an application error.
    ApplicationError,
    /// A hard timeout stopped the attempt.
    TimedOut { scope: RetryTimeoutScope },
    /// The isolated attempt panicked.
    Panicked { panic: RetryPanic },
}
