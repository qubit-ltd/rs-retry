// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private backoff policy representation types.

mod backoff_policy_data;
mod backoff_strategy;
mod jitter_strategy;
mod retry_after_strategy;

#[cfg(feature = "serde")]
pub(super) use backoff_policy_data::BackoffPolicyData;
pub(super) use backoff_strategy::BackoffStrategy;
#[cfg(feature = "serde")]
pub(super) use backoff_strategy::BackoffStrategyData;
pub(super) use jitter_strategy::JitterStrategy;
#[cfg(feature = "serde")]
pub(super) use jitter_strategy::JitterStrategyData;
pub(super) use retry_after_strategy::RetryAfterStrategy;
#[cfg(feature = "serde")]
pub(super) use retry_after_strategy::RetryAfterStrategyData;
