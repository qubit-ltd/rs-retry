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

pub(super) use backoff_policy_data::BackoffPolicyData;
pub(super) use backoff_strategy::BackoffStrategy;
pub(super) use jitter_strategy::JitterStrategy;
pub(super) use retry_after_strategy::RetryAfterStrategy;
