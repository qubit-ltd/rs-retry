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
//!
//! The public workflow is intentionally small:
//!
//! 1. Build a [`Retry`] policy with [`Retry::builder`] or
//!    [`Retry::from_options`].
//! 2. Choose the execution mode:
//!    - [`Retry::run`] for low-overhead same-thread synchronous work.
//!    - `Retry::run_async` for Tokio futures and async timeouts when the
//!      `tokio` feature is enabled.
//!    - [`Retry::run_in_worker`] for blocking work that needs panic capture,
//!      timeout waiting, or cooperative cancellation.
//! 3. Inspect [`RetryError`] when the flow stops. It keeps the terminal reason,
//!    the last observed [`AttemptFailure`], and the final [`RetryContext`].
//!
//! Internally, `Retry` stays a facade. Options, event dispatch, flow state,
//! failure policy, and execution loops live in separate objects so each piece
//! owns one retry concern.

pub mod constants;
pub mod error;
pub mod event;
pub mod executor;
pub mod options;

pub use error::{
    AttemptExecutorError,
    AttemptFailure,
    AttemptPanic,
    RetryConfigError,
    RetryError,
    RetryErrorReason,
    RetryResult,
};
pub use event::{
    AttemptFailureDecision,
    AttemptFailureListener,
    AttemptSuccessListener,
    AttemptTimeoutSource,
    BeforeAttemptListener,
    RetryAfterHint,
    RetryContext,
    RetryErrorListener,
    RetryScheduledListener,
};
pub use executor::{
    AttemptCancelToken,
    Retry,
    RetryBuilder,
};
#[cfg(feature = "config")]
pub use options::RetryConfigValues;
pub use options::{
    AttemptTimeoutOption,
    AttemptTimeoutPolicy,
    ParseRetryJitterError,
    RetryDelay,
    RetryJitter,
    RetryOptions,
};
