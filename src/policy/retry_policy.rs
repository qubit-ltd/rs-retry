// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable retry policy.

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Deserializer;
#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "serde")]
use serde::Serializer;

use super::RetryLimits;
use super::RetryPolicyBuilder;
#[cfg(feature = "serde")]
use super::internal::RetryPolicyData;
use crate::backoff::BackoffPolicy;

/// Pure retry limits and backoff configuration.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    limits: RetryLimits,
    backoff: BackoffPolicy,
}

#[cfg(feature = "serde")]
impl Serialize for RetryPolicy {
    /// Serializes a policy through its stable private wire DTO.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RetryPolicyData::from(self).serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for RetryPolicy {
    /// Deserializes a policy and validates all represented configuration.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = RetryPolicyData::deserialize(deserializer)?;
        Self::try_from(data).map_err(serde::de::Error::custom)
    }
}

impl RetryPolicy {
    /// Creates a policy builder.
    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::new()
    }

    /// Creates a policy from validated components.
    pub(crate) fn new(limits: RetryLimits, backoff: BackoffPolicy) -> Self {
        Self { limits, backoff }
    }

    /// Returns retry continuation limits.
    #[must_use = "inspect the retry limits"]
    pub fn limits(&self) -> &RetryLimits {
        &self.limits
    }

    /// Returns the immutable backoff configuration.
    #[must_use = "inspect the backoff configuration"]
    pub fn backoff(&self) -> &BackoffPolicy {
        &self.backoff
    }
}
