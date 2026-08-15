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
#[cfg(feature = "serde")]
mod backoff_strategy_data;
mod jitter_strategy;
#[cfg(feature = "serde")]
mod jitter_strategy_data;
mod retry_after_strategy;
#[cfg(feature = "serde")]
mod retry_after_strategy_data;

#[cfg(feature = "serde")]
pub(super) use backoff_policy_data::BackoffPolicyData;
pub(super) use backoff_strategy::BackoffStrategy;
#[cfg(feature = "serde")]
pub(super) use backoff_strategy_data::BackoffStrategyData;
pub(super) use jitter_strategy::JitterStrategy;
#[cfg(feature = "serde")]
pub(super) use jitter_strategy_data::JitterStrategyData;
pub(super) use retry_after_strategy::RetryAfterStrategy;
#[cfg(feature = "serde")]
pub(super) use retry_after_strategy_data::RetryAfterStrategyData;
