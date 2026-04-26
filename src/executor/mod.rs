/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry executor and builder modules and public re-exports.

#[cfg(feature = "tokio")]
mod async_attempt;
#[cfg(feature = "tokio")]
mod async_attempt_future;
#[cfg(feature = "tokio")]
mod async_value_operation;
mod attempt_cancel_token;
mod blocking_attempt_message;
mod retry;
mod retry_builder;
mod retry_flow_action;
mod sync_attempt;
mod sync_value_operation;

pub use attempt_cancel_token::AttemptCancelToken;
pub use retry::Retry;
#[cfg(all(coverage, not(test)))]
#[doc(hidden)]
pub use retry::coverage_support;
pub use retry_builder::RetryBuilder;
