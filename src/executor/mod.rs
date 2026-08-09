// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry executor and builder modules and public re-exports.

#[cfg(feature = "tokio")]
mod async_retry;
mod attempt_cancel_token;
mod blocking_attempt;
mod blocking_attempt_outcome;
mod blocking_value_operation;
mod retry;
mod retry_builder;
mod retry_success;
mod sync_retry;
mod worker_attempt_executor;
mod worker_retry;

#[cfg(feature = "tokio")]
pub use async_retry::AsyncRetry;
pub use attempt_cancel_token::AttemptCancelToken;
pub use retry::Retry;
pub use retry_builder::RetryBuilder;
pub use retry_success::RetrySuccess;
pub use sync_retry::SyncRetry;
pub use worker_retry::WorkerRetry;
