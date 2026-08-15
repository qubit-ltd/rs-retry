// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private base-delay strategy representation.

use std::time::Duration;

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "serde")]
use crate::policy::internal::DurationData;

/// Base-delay strategy used by [`super::super::BackoffPolicy`].
#[derive(Debug, Clone, PartialEq)]
pub enum BackoffStrategy {
    /// No delay.
    Immediate,
    /// A fixed delay.
    Fixed {
        /// Delay applied to each retry.
        delay: Duration,
    },
    /// A uniformly sampled delay range.
    Uniform {
        /// Inclusive lower bound.
        min: Duration,
        /// Inclusive upper bound.
        max: Duration,
    },
    /// A capped exponential delay.
    Exponential {
        /// Delay used for the first retry.
        initial: Duration,
        /// Multiplicative factor applied between retries.
        multiplier: f64,
        /// Maximum delay.
        max: Duration,
    },
}

/// Stable serde representation of a base-delay strategy.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BackoffStrategyData {
    /// No delay.
    Immediate,
    /// A fixed delay.
    Fixed {
        /// Delay applied to each retry.
        delay: DurationData,
    },
    /// A uniformly sampled delay range.
    Uniform {
        /// Inclusive lower bound.
        minimum: DurationData,
        /// Inclusive upper bound.
        maximum: DurationData,
    },
    /// A capped exponential delay.
    Exponential {
        /// Delay used for the first retry.
        initial: DurationData,
        /// Multiplicative factor applied between retries.
        multiplier: f64,
        /// Maximum delay.
        maximum: DurationData,
    },
}

#[cfg(feature = "serde")]
impl From<&BackoffStrategy> for BackoffStrategyData {
    /// Converts a runtime strategy to its stable wire representation.
    fn from(strategy: &BackoffStrategy) -> Self {
        match strategy {
            BackoffStrategy::Immediate => Self::Immediate,
            BackoffStrategy::Fixed { delay } => Self::Fixed {
                delay: DurationData::from(*delay),
            },
            BackoffStrategy::Uniform { min, max } => Self::Uniform {
                minimum: DurationData::from(*min),
                maximum: DurationData::from(*max),
            },
            BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            } => Self::Exponential {
                initial: DurationData::from(*initial),
                multiplier: *multiplier,
                maximum: DurationData::from(*max),
            },
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<BackoffStrategyData> for BackoffStrategy {
    type Error = crate::RetryPolicyError;

    /// Converts a wire strategy while checking all encoded durations.
    fn try_from(data: BackoffStrategyData) -> Result<Self, Self::Error> {
        match data {
            BackoffStrategyData::Immediate => Ok(Self::Immediate),
            BackoffStrategyData::Fixed { delay } => Ok(Self::Fixed {
                delay: delay.try_into()?,
            }),
            BackoffStrategyData::Uniform { minimum, maximum } => {
                Ok(Self::Uniform {
                    min: minimum.try_into()?,
                    max: maximum.try_into()?,
                })
            }
            BackoffStrategyData::Exponential {
                initial,
                multiplier,
                maximum,
            } => Ok(Self::Exponential {
                initial: initial.try_into()?,
                multiplier,
                max: maximum.try_into()?,
            }),
        }
    }
}
