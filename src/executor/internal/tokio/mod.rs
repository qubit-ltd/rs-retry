//! Shared asynchronous retry outcomes.

mod async_attempt_outcome;
mod async_backoff_outcome;

pub(in crate::executor) use async_attempt_outcome::AsyncAttemptOutcome;
pub(in crate::executor) use async_backoff_outcome::AsyncBackoffOutcome;
