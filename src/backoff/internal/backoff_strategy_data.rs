// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable serde representation of the base-delay strategy.

use serde::Deserialize;
use serde::Serialize;

use super::BackoffStrategy;
use crate::policy::internal::DurationData;

/// Stable serde representation of a base-delay strategy.
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

impl From<&BackoffStrategy> for BackoffStrategyData {
    /// Converts a runtime strategy to its stable wire representation.
    fn from(strategy: &BackoffStrategy) -> Self {
        match strategy {
            BackoffStrategy::Immediate => Self::Immediate,
            BackoffStrategy::Fixed { delay } => Self::Fixed {
                delay: (*delay).into(),
            },
            BackoffStrategy::Uniform { min, max } => Self::Uniform {
                minimum: (*min).into(),
                maximum: (*max).into(),
            },
            BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            } => Self::Exponential {
                initial: (*initial).into(),
                multiplier: *multiplier,
                maximum: (*max).into(),
            },
        }
    }
}

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
