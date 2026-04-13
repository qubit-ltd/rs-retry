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
mod error_classifier;
mod retry_config_error;
mod retry_error;

pub use attempt_failure::AttemptFailure;
pub use error_classifier::ErrorClassifier;
pub use retry_config_error::RetryConfigError;
pub use retry_error::RetryError;
