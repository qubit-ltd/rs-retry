// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Internal location for the retry context value.

mod attempt_failure_decision;
mod attempt_timeout_source;
mod retry_context;
mod retry_context_parts;

pub use attempt_failure_decision::AttemptFailureDecision;
pub use attempt_timeout_source::AttemptTimeoutSource;
pub use retry_context::RetryContext;
pub(crate) use retry_context_parts::RetryContextParts;
