// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Terminal results produced by retry executors.

mod retry_error;
mod retry_error_metadata;
mod retry_error_reason;
mod retry_result;
mod retry_success;

pub use retry_error::RetryError;
pub use retry_error_metadata::RetryErrorMetadata;
pub use retry_error_reason::RetryErrorReason;
pub use retry_result::RetryResult;
pub use retry_success::RetrySuccess;
