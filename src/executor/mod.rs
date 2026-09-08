// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry executor and builder modules and public re-exports.

#[cfg(feature = "tokio")]
mod tokio_retry;
#[cfg(feature = "worker")]
mod attempt_cancellation_token;
mod internal;
mod retry;
mod retry_builder;
mod retry_cancellation_token;
mod retry_cancelled;
mod retry_success;
mod sync_retry;
#[cfg(feature = "worker")]
mod worker_retry;

#[cfg(feature = "tokio")]
pub use tokio_retry::TokioRetry;
#[cfg(feature = "worker")]
pub use attempt_cancellation_token::AttemptCancellationToken;
pub use retry::Retry;
pub use retry_builder::RetryBuilder;
pub use retry_cancellation_token::RetryCancellationToken;
pub use retry_cancelled::RetryCancelled;
pub use retry_success::RetrySuccess;
pub use sync_retry::SyncRetry;
#[cfg(feature = "worker")]
pub use worker_retry::WorkerRetry;
