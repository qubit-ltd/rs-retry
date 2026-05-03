/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Error types used by retry executors.

mod attempt_executor_error;
mod attempt_failure;
mod attempt_panic;
mod retry_config_error;
mod retry_error;
mod retry_error_reason;

pub use attempt_executor_error::AttemptExecutorError;
pub use attempt_failure::AttemptFailure;
pub use attempt_panic::AttemptPanic;
pub use retry_config_error::RetryConfigError;
pub use retry_error::{RetryError, RetryResult};
pub use retry_error_reason::RetryErrorReason;
