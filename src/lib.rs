// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Type-preserving retry policy for synchronous, asynchronous, and worker
//! thread operations.
//!
//! Build a [`RetryPolicy`] once, attach ordered [`RetryRule`] values and
//! [`RetryObserver`] values through [`Retry::builder`], then select the
//! execution facade that matches the operation. A policy only decides whether
//! another attempt may be admitted; success always wins, even when an attempt
//! completes after a budget boundary.

pub mod backoff;
pub mod budget;
pub mod error;
mod event;
pub mod executor;
pub mod observer;
pub mod policy;
pub mod random;
pub mod rule;

pub use backoff::BackoffDelaySource;
pub use backoff::BackoffPolicy;
pub use backoff::BackoffRequest;
pub use backoff::BackoffState;
pub use backoff::BackoffStep;
pub use budget::RetryAttempt;
pub use budget::RetryBudget;
pub use budget::RetryBudgetError;
pub use budget::RetryBudgetExhausted;
pub use budget::RetryBudgetSnapshot;
pub(crate) use error::AttemptExecutionError;
pub use error::AttemptFailure;
pub use error::AttemptFailureKind;
pub use error::AttemptTimeoutKind;
pub use error::RetryCallbackFailure;
pub use error::RetryCallbackKind;
pub use error::RetryCallbackPhase;
pub use error::RetryCancellationPhase;
pub use error::RetryError;
pub use error::RetryErrorKind;
pub use error::RetryErrorReason;
pub(crate) use error::RetryExecutionError;
pub use error::RetryFailure;
pub use error::RetryInfrastructureFailure;
pub use error::RetryLimitKind;
pub use error::RetryPanic;
pub use error::RetryPolicyError;
pub use error::RetryResult;
pub use error::RetryTimeoutScope;
pub use error::WorkerStopTrigger;
#[cfg(feature = "tokio")]
pub use executor::AsyncRetry;
pub use executor::AttemptCancelToken;
pub use executor::Retry;
pub use executor::RetryBuilder;
pub use executor::RetrySuccess;
pub use executor::WorkerRetry;
pub use observer::RetryContext;
pub use observer::RetryDiagnostic;
pub use observer::RetryDiagnosticKind;
pub use observer::RetryObserver;
pub use observer::RetryOutcomeKind;
pub use policy::RetryLimits;
pub use policy::RetryPolicy;
pub use policy::RetryPolicyBuilder;
pub use random::RetryRandomSource;
pub use rule::RetryDecision;
pub use rule::RetryRule;
