// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared implementation state for retry executors.

mod attempt_plan;
mod effective_timeout;
#[cfg(feature = "tokio")]
mod prepared_attempt_plan;
#[cfg(feature = "tokio")]
mod prepared_timeout;
mod retry_cancellation_state;
mod retry_directive;
mod retry_flow_controller;
mod retry_flow_state;
mod waker_registry;

pub(crate) use attempt_plan::AttemptPlan;
pub(crate) use effective_timeout::EffectiveTimeout;
#[cfg(feature = "tokio")]
pub(crate) use prepared_attempt_plan::PreparedAttemptPlan;
pub(in crate::executor) use retry_cancellation_state::RetryCancellationState;
pub(crate) use retry_directive::RetryDirective;
pub(crate) use retry_flow_controller::RetryFlowController;
pub(crate) use retry_flow_state::RetryFlowState;
