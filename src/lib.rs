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
//! [`RetryObserver`] values through [`RetryConfig::builder`], then construct
//! the execution facade that matches the operation. A policy only decides
//! whether another attempt may be admitted; an admitted success is not revoked
//! by a soft budget. Cancellation and hard-timeout priorities depend on the
//! selected facade, and completion requires valid clock accounting.

pub mod backoff;
pub mod budget;
mod context;
pub mod error;
pub mod executor;
mod internal;
pub mod observer;
mod outcome;
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
pub use context::RetryContext;
pub use error::AttemptFailure;
pub use error::AttemptFailureMetadata;
pub use error::RetryCallbackFailure;
pub use error::RetryCallbackKind;
pub use error::RetryCallbackPhase;
pub use error::RetryCancellationPhase;
pub use error::RetryError;
pub use error::RetryErrorMetadata;
pub use error::RetryErrorReason;
pub use error::RetryInfrastructureFailure;
pub use error::RetryLimitKind;
pub use error::RetryPanic;
pub use error::RetryPolicyError;
pub use error::RetryResult;
pub use error::RetryTimeoutScope;
#[cfg(feature = "worker")]
pub use error::WorkerStopTrigger;
#[cfg(feature = "worker")]
pub use executor::AttemptCancellationToken;
pub use executor::Retry;
pub use executor::RetryCancellationToken;
pub use executor::RetryCancelled;
pub use executor::RetryConfig;
pub use executor::RetryConfigBuilder;
pub use executor::RetrySuccess;
#[cfg(feature = "tokio")]
pub use executor::TokioRetry;
#[cfg(feature = "worker")]
pub use executor::WorkerRetry;
pub use observer::RetryObserver;
pub use policy::RetryAdmissionLimits;
pub use policy::RetryPolicy;
pub use policy::RetryPolicyBuilder;
pub use random::RetryRandomSource;
pub use rule::RetryDecision;
pub use rule::RetryFallback;
pub use rule::RetryRule;
