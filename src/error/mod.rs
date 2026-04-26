/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Error types used by retry executors.

mod attempt_failure;
mod attempt_panic;
mod retry_config_error;
mod retry_error;
mod retry_error_reason;

pub use attempt_failure::AttemptFailure;
pub use attempt_panic::AttemptPanic;
pub use retry_config_error::RetryConfigError;
pub use retry_error::RetryError;
pub use retry_error_reason::RetryErrorReason;
