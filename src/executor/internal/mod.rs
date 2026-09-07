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
mod blocking_attempt;
mod blocking_attempt_outcome;
mod blocking_backoff;
mod blocking_backoff_outcome;
mod blocking_backoff_wake;
mod blocking_value_operation;
mod effective_timeout;
mod prepared_attempt_plan;
mod prepared_timeout;
mod retry_cancellation_state;
mod retry_directive;
mod retry_flow_controller;
mod retry_flow_state;
mod waker_registry;
mod worker_attempt_executor;
mod worker_event;
mod worker_wake;

#[cfg(feature = "tokio")]
pub(in crate::executor) use async_attempt_outcome::AsyncAttemptOutcome;
#[cfg(feature = "tokio")]
pub(in crate::executor) use async_backoff_outcome::AsyncBackoffOutcome;
pub(in crate::executor) use blocking_attempt::BlockingAttempt;
pub(in crate::executor) use blocking_attempt_outcome::BlockingAttemptOutcome;
pub(crate) use blocking_backoff::wait_for_backoff;
pub(crate) use blocking_backoff_outcome::BlockingBackoffOutcome;
pub(in crate::executor) use blocking_value_operation::BlockingValueOperation;
pub(crate) use effective_timeout::EffectiveTimeout;
pub(crate) use prepared_attempt_plan::PreparedAttemptPlan;
pub(in crate::executor) use retry_cancellation_state::RetryCancellationState;
pub(crate) use retry_directive::RetryDirective;
pub(crate) use retry_flow_controller::RetryFlowController;
pub(crate) use retry_flow_state::RetryFlowState;
pub(in crate::executor) use worker_attempt_executor::WorkerAttemptExecutor;
