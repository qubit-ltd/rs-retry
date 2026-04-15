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

pub mod error;
pub mod event;
pub mod options;

mod retry_executor;
mod retry_executor_builder;

pub use options::{RetryDelay, RetryJitter, RetryOptions};
pub use error::{RetryAttemptFailure, RetryErrorClassifier, RetryConfigError, RetryError};
pub use event::{
    RetryAbortContext, RetryAbortListener, RetryAttemptContext, RetryFailureContext, RetryFailureListener, RetryContext,
    RetryDecision, RetryListener, RetrySuccessContext, RetrySuccessListener,
};
pub use retry_executor::RetryExecutor;
pub use retry_executor_builder::RetryExecutorBuilder;

/// Result alias returned by retry executor execution.
///
/// The success type `T` is chosen by each operation. The error type `E`
/// remains the caller's original application error and is wrapped by
/// [`RetryError`] only when retry execution terminates unsuccessfully.
pub type RetryResult<T, E> = Result<T, RetryError<E>>;
