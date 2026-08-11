// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Immutable retry policy.

#[cfg(feature = "config")]
use qubit_config::ConfigReader;
use serde::Deserialize;
use serde::Serialize;

use super::RetryLimits;
use super::RetryPolicyBuilder;
#[cfg(feature = "config")]
use crate::RetryConfigError;
#[cfg(feature = "config")]
use crate::RetryOptions;
use crate::backoff::BackoffPolicy;

/// Pure retry limits and backoff configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    limits: RetryLimits,
    backoff: BackoffPolicy,
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
    pub fn limits(&self) -> &RetryLimits {
        &self.limits
    }

    /// Returns the immutable backoff configuration.
    pub fn backoff(&self) -> &BackoffPolicy {
        &self.backoff
    }

    /// Loads the legacy configuration shape and converts it into a pure
    /// policy. This keeps configuration parsing at the boundary; executors
    /// never read config keys during execution.
    #[cfg(feature = "config")]
    pub fn from_config<R>(config: &R) -> Result<Self, RetryConfigError>
    where
        R: ConfigReader + ?Sized,
    {
        RetryOptions::from_config(config)?
            .to_policy()
            .map_err(|error| {
                RetryConfigError::invalid_value(error.field(), error.message())
            })
    }
}
