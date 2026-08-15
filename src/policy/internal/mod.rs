// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private serde wire DTOs for validated retry configuration.

#[cfg(feature = "serde")]
mod duration_data;
#[cfg(feature = "serde")]
mod retry_limits_data;
#[cfg(feature = "serde")]
mod retry_policy_data;

#[cfg(feature = "serde")]
pub(crate) use duration_data::DurationData;
#[cfg(feature = "serde")]
pub(crate) use retry_limits_data::RetryLimitsData;
#[cfg(feature = "serde")]
pub(crate) use retry_policy_data::RetryPolicyData;
