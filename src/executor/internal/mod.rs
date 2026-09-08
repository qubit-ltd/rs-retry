// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared implementation state for retry executors.

mod blocking;
mod cancellation;
mod flow;
#[cfg(feature = "tokio")]
mod tokio;
#[cfg(feature = "worker")]
mod worker;

pub(crate) use blocking::BlockingBackoffOutcome;
pub(crate) use blocking::wait_for_backoff;
pub(in crate::executor) use cancellation::RetryCancellationState;
pub(crate) use flow::RetryFlowController;
#[cfg(feature = "tokio")]
pub(in crate::executor) use tokio::AsyncAttemptOutcome;
#[cfg(feature = "tokio")]
pub(in crate::executor) use tokio::AsyncBackoffOutcome;
#[cfg(feature = "worker")]
pub(in crate::executor) use worker::BlockingAttempt;
#[cfg(feature = "worker")]
pub(in crate::executor) use worker::BlockingAttemptOutcome;
#[cfg(feature = "worker")]
pub(in crate::executor) use worker::BlockingValueOperation;
#[cfg(feature = "worker")]
pub(in crate::executor) use worker::WorkerAttemptExecutor;
