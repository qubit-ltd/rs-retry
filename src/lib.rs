/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Type-preserving retry executors for synchronous and asynchronous operations.
//!
//! `RetryExecutor<E>` binds only the operation error type. The success type `T`
//! is introduced on `run` / `run_async`, so normal error retry does not require
//! `T: Clone + Eq + Hash`.
//!
//! The default error type is `BoxError` from the `qubit-common` crate. It is not
//! re-exported by this crate; callers that need the boxed error alias should
//! import it from `qubit-common` directly.

pub mod delay;
pub mod error;
pub mod events;
pub mod jitter;

mod failure_action;
mod retry_executor;
mod retry_executor_builder;
mod retry_options;

pub use delay::Delay;
pub use error::{AttemptFailure, ErrorClassifier, RetryConfigError, RetryError};
pub use events::{
    AbortEvent, AbortListener, AttemptContext, FailureEvent, FailureListener, RetryDecision,
    RetryEvent, RetryListener, SuccessEvent, SuccessListener,
};
pub use jitter::Jitter;
pub use retry_executor::RetryExecutor;
pub use retry_executor_builder::RetryExecutorBuilder;
pub use retry_options::RetryOptions;

/// Result alias returned by retry executor execution.
///
/// The success type `T` is chosen by each operation. The error type `E`
/// remains the caller's original application error and is wrapped by
/// [`RetryError`] only when retry execution terminates unsuccessfully.
pub type RetryResult<T, E> = Result<T, RetryError<E>>;
