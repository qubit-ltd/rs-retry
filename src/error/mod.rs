// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Error types used by retry executors.

mod attempt_execution_error;
mod attempt_failure;
mod attempt_failure_kind;
mod attempt_timeout_kind;
mod retry_callback_failure;
mod retry_callback_kind;
mod retry_callback_phase;
mod retry_cancellation_phase;
mod retry_error;
mod retry_error_kind;
mod retry_error_reason;
mod retry_execution_error;
mod retry_execution_error_kind;
mod retry_failure;
mod retry_infrastructure_failure;
mod retry_limit_kind;
mod retry_panic;
mod retry_policy_error;
mod retry_timeout_scope;
mod worker_stop_trigger;

pub use attempt_execution_error::AttemptExecutionError;
pub use attempt_failure::AttemptFailure;
pub use attempt_failure_kind::AttemptFailureKind;
pub use attempt_timeout_kind::AttemptTimeoutKind;
pub use retry_callback_failure::RetryCallbackFailure;
pub use retry_callback_kind::RetryCallbackKind;
pub use retry_callback_phase::RetryCallbackPhase;
pub use retry_cancellation_phase::RetryCancellationPhase;
pub use retry_error::RetryError;
pub use retry_error::RetryResult;
pub use retry_error_kind::RetryErrorKind;
pub use retry_error_reason::RetryErrorReason;
pub use retry_execution_error::RetryExecutionError;
pub use retry_execution_error_kind::RetryExecutionErrorKind;
pub use retry_failure::RetryFailure;
pub use retry_infrastructure_failure::RetryInfrastructureFailure;
pub use retry_limit_kind::RetryLimitKind;
pub use retry_panic::RetryPanic;
pub use retry_policy_error::RetryPolicyError;
pub use retry_timeout_scope::RetryTimeoutScope;
pub use worker_stop_trigger::WorkerStopTrigger;
