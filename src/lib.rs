/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Type-preserving retry policy for synchronous and asynchronous operations.
//!
//! `Retry<E>` binds only the operation error type. The success type `T` is
//! introduced on `run` / `run_async`, so normal error retry does not require
//! `T: Clone + Eq + Hash`.
//!
//! The default error type is `BoxError` from the `qubit-common` crate. It is not
//! re-exported by this crate; callers that need the boxed error alias should
//! import it from `qubit-common` directly.

pub mod constants;
pub mod error;
pub mod event;
pub mod executor;
pub mod options;

pub use error::{
    AttemptExecutorError, AttemptFailure, AttemptPanic, RetryConfigError, RetryError,
    RetryErrorReason,
};
pub use event::{
    AttemptFailureDecision, AttemptFailureListener, AttemptSuccessListener, BeforeAttemptListener,
    RetryAfterHint, RetryContext, RetryErrorListener, RetryScheduledListener,
};
pub use executor::{AttemptCancelToken, Retry, RetryBuilder};
#[cfg(feature = "config")]
pub use options::RetryConfigValues;
pub use options::{
    AttemptTimeoutOption, AttemptTimeoutPolicy, ParseRetryJitterError, RetryDelay, RetryJitter,
    RetryOptions,
};

/// Result alias returned by retry executor execution.
///
/// The success type `T` is chosen by each operation. The error type `E`
/// remains the caller's original application error and is wrapped by
/// [`RetryError`] only when retry execution terminates unsuccessfully.
pub type RetryResult<T, E> = Result<T, RetryError<E>>;
