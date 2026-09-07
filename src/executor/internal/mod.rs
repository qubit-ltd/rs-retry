// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared implementation state for retry executors.

#[cfg(feature = "tokio")]
mod async_attempt_outcome;
#[cfg(feature = "tokio")]
mod async_backoff_outcome;
#[cfg(feature = "worker")]
mod blocking_attempt;
#[cfg(feature = "worker")]
mod blocking_attempt_outcome;
mod blocking_backoff;
mod blocking_backoff_outcome;
mod blocking_backoff_wake;
#[cfg(feature = "worker")]
mod blocking_value_operation;
mod effective_timeout;
mod prepared_attempt_plan;
mod prepared_backoff_plan;
mod prepared_timeout;
mod retry_cancellation_state;
mod retry_flow_controller;
mod retry_flow_state;
mod waker_registry;
#[cfg(feature = "worker")]
mod worker_attempt_executor;
#[cfg(feature = "worker")]
mod worker_event;
#[cfg(feature = "worker")]
mod worker_wake;

#[cfg(feature = "tokio")]
pub(in crate::executor) use async_attempt_outcome::AsyncAttemptOutcome;
#[cfg(feature = "tokio")]
pub(in crate::executor) use async_backoff_outcome::AsyncBackoffOutcome;
#[cfg(feature = "worker")]
pub(in crate::executor) use blocking_attempt::BlockingAttempt;
#[cfg(feature = "worker")]
pub(in crate::executor) use blocking_attempt_outcome::BlockingAttemptOutcome;
pub(crate) use blocking_backoff::wait_for_backoff;
pub(crate) use blocking_backoff_outcome::BlockingBackoffOutcome;
#[cfg(feature = "worker")]
pub(in crate::executor) use blocking_value_operation::BlockingValueOperation;
pub(crate) use effective_timeout::EffectiveTimeout;
pub(crate) use prepared_attempt_plan::PreparedAttemptPlan;
pub(crate) use prepared_backoff_plan::PreparedBackoffPlan;
pub(in crate::executor) use retry_cancellation_state::RetryCancellationState;
pub(crate) use retry_flow_controller::RetryFlowController;
pub(crate) use retry_flow_state::RetryFlowState;
#[cfg(feature = "worker")]
pub(in crate::executor) use worker_attempt_executor::WorkerAttemptExecutor;
