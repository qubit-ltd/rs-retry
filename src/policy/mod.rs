// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Pure retry policy values and their validated builder.

pub(crate) mod internal;
mod retry_limits;
mod retry_policy;
mod retry_policy_builder;

pub use retry_limits::RetryLimits;
pub use retry_policy::RetryPolicy;
pub use retry_policy_builder::RetryPolicyBuilder;
