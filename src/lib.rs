// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
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

pub mod backoff;
pub mod constants;
pub mod error;
pub mod event;
pub mod executor;
pub mod observer;
pub mod options;
pub mod policy;
pub mod random;
pub mod rule;

pub use backoff::BackoffDelaySource;
pub use backoff::BackoffPolicy;
pub use backoff::BackoffRequest;
pub use backoff::BackoffState;
pub use backoff::BackoffStep;
pub use error::AttemptExecutionError;
pub use error::AttemptExecutorError;
pub use error::AttemptFailure;
pub use error::AttemptFailureKind;
pub use error::AttemptPanic;
pub use error::AttemptTimeoutKind;
pub use error::RetryConfigError;
pub use error::RetryError;
pub use error::RetryErrorKind;
pub use error::RetryErrorReason;
pub use error::RetryExecutionError;
pub use error::RetryExecutionErrorKind;
pub use error::RetryPolicyError;
pub use error::RetryResult;
pub use event::AttemptFailureDecision;
pub use event::AttemptTimeoutSource;
pub use event::RetryContext;
pub use executor::AttemptCancelToken;
pub use executor::Retry;
pub use executor::RetryBuilder;
pub use executor::RetrySuccess;
pub use observer::RetryDiagnostic;
pub use observer::RetryDiagnosticKind;
pub use observer::RetryObserver;
pub use observer::RetryOutcomeKind;
pub use options::AttemptTimeoutOption;
pub use options::AttemptTimeoutPolicy;
pub use options::ParseRetryJitterError;
pub use options::RetryAfterPolicy;
#[cfg(feature = "config")]
pub use options::RetryConfigValues;
pub use options::RetryDelay;
pub use options::RetryJitter;
pub use options::RetryOptions;
pub use options::RetryOptionsBuilder;
pub use policy::RetryLimits;
pub use policy::RetryPolicy;
pub use policy::RetryPolicyBuilder;
pub use random::RetryRandomSource;
pub use rule::RetryDecision;
pub use rule::RetryRule;
