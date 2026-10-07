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
#[cfg(feature = "serde")]
use serde::de::Error;

use super::RetryAdmissionLimits;
use super::RetryPolicyBuilder;
#[cfg(feature = "serde")]
use super::internal::RetryPolicyData;
use crate::backoff::BackoffPolicy;

/// Pure retry limits and backoff configuration.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_retry::BackoffPolicy;
/// use qubit_retry::RetryPolicy;
///
/// let policy = RetryPolicy::builder()
///     .max_attempts(4)
///     .total_time_budget(Duration::from_secs(10))
///     .backoff(BackoffPolicy::fixed(Duration::from_millis(50)))
///     .build()?;
/// assert_eq!(policy.admission_limits().max_attempts().get(), 4);
/// assert_eq!(policy.backoff().maximum_delay(), Some(Duration::from_millis(50)));
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Validated admission and elapsed-time budgets.
    limits: RetryAdmissionLimits,
    /// Validated delay configuration copied into each fresh flow.
    backoff: BackoffPolicy,
}

#[cfg(feature = "serde")]
impl Serialize for RetryPolicy {
    /// Serializes a policy through its stable private wire DTO.
    ///
    /// # Type Parameters
    /// - `S`: Serializer for the stable configuration representation.
    ///
    /// # Parameters
    /// - `serializer`: Serialization destination, consumed by this call.
    ///
    /// # Returns
    /// The serializer output for the validated configuration.
    ///
    /// # Errors
    /// Returns any serialization error reported by the destination.
    #[inline(always)]
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
    ///
    /// # Type Parameters
    /// - `D`: Deserializer borrowing input for its declared lifetime.
    ///
    /// # Parameters
    /// - `deserializer`: Source of the stable wire configuration.
    ///
    /// # Returns
    /// A validated configuration value.
    ///
    /// # Errors
    /// Rejects malformed input, unknown fields and invalid policy/duration
    /// values.
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = RetryPolicyData::deserialize(deserializer)?;
        Self::try_from(data).map_err(Error::custom)
    }
}

impl RetryPolicy {
    /// Creates a policy builder.
    ///
    /// # Returns
    /// A builder defaulting to three attempts, immediate backoff and no elapsed
    /// limits.
    #[inline(always)]
    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::new()
    }

    /// Creates a policy from validated components.
    ///
    /// # Parameters
    /// - `limits`: Already validated continuation limits.
    /// - `backoff`: Already validated backoff configuration.
    ///
    /// # Returns
    /// An immutable policy owning both components.
    #[inline]
    pub(crate) fn new(limits: RetryAdmissionLimits, backoff: BackoffPolicy) -> Self {
        Self { limits, backoff }
    }

    /// Returns retry continuation limits.
    ///
    /// # Returns
    /// Borrowed immutable limits; inspecting them does not start a flow.
    #[must_use = "inspect the retry limits"]
    #[inline(always)]
    pub fn admission_limits(&self) -> &RetryAdmissionLimits {
        &self.limits
    }

    /// Returns the immutable backoff configuration.
    ///
    /// # Returns
    /// Borrowed immutable delay policy shared conceptually across executions.
    #[must_use = "inspect the backoff configuration"]
    #[inline(always)]
    pub fn backoff(&self) -> &BackoffPolicy {
        &self.backoff
    }
}
