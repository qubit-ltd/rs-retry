// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Error types used by retry executors.

mod attempt_execution_error;
mod attempt_executor_error;
mod attempt_failure;
mod attempt_failure_kind;
mod attempt_panic;
mod attempt_timeout_kind;
mod retry_config_error;
mod retry_error;
mod retry_error_kind;
mod retry_error_reason;
mod retry_execution_error;
mod retry_policy_error;

pub use attempt_execution_error::AttemptExecutionError;
pub use attempt_executor_error::AttemptExecutorError;
pub use attempt_failure::AttemptFailure;
pub use attempt_failure_kind::AttemptFailureKind;
pub use attempt_panic::AttemptPanic;
pub use attempt_timeout_kind::AttemptTimeoutKind;
pub use retry_config_error::RetryConfigError;
pub(crate) use retry_config_error::argument_error_message;
pub use retry_error::RetryError;
pub use retry_error::RetryResult;
pub use retry_error_kind::RetryErrorKind;
pub use retry_error_reason::RetryErrorReason;
pub use retry_execution_error::RetryExecutionError;
pub use retry_execution_error::RetryExecutionErrorKind;
pub use retry_policy_error::RetryPolicyError;
