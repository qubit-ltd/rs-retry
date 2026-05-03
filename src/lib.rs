/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Type-preserving retry policy for synchronous and asynchronous operations.
//!
//! `Retry<E>` binds only the operation error type. The success type `T` is
//! introduced on `run` / `run_async`, so normal error retry does not require
//! `T: Clone + Eq + Hash`.
//!
//! The default error type is `BoxError` from the `qubit-error` crate. It is not
//! re-exported by this crate; callers that need the boxed error alias should
//! import it from `qubit-error` directly.

pub mod constants;
pub mod error;
pub mod event;
pub mod executor;
pub mod options;

pub use error::{
    AttemptExecutorError, AttemptFailure, AttemptPanic, RetryConfigError, RetryError,
    RetryErrorReason, RetryResult,
};
pub use event::{
    AttemptFailureDecision, AttemptFailureListener, AttemptSuccessListener, AttemptTimeoutSource,
    BeforeAttemptListener, RetryAfterHint, RetryContext, RetryErrorListener,
    RetryScheduledListener,
};
pub use executor::{AttemptCancelToken, Retry, RetryBuilder};
#[cfg(feature = "config")]
pub use options::RetryConfigValues;
pub use options::{
    AttemptTimeoutOption, AttemptTimeoutPolicy, ParseRetryJitterError, RetryDelay, RetryJitter,
    RetryOptions,
};
