// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable serde representation of the jitter strategy.

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::de::Error;

use super::JitterStrategy;
use super::jitter_strategy_data_raw::JitterStrategyDataRaw;
use super::jitter_strategy_tag::JitterStrategyTag;
use super::ratio_field::RatioField;

/// Stable serde representation of a jitter strategy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum JitterStrategyData {
    /// Do not vary the selected delay.
    None,
    /// Sample from zero through the selected delay.
    Full,
    /// Apply a symmetric multiplicative range.
    Bounded {
        /// Maximum relative deviation.
        ratio: f64,
    },
}

impl<'de> Deserialize<'de> for JitterStrategyData {
    /// Deserializes one jitter strategy and rejects irrelevant ratio fields.
    ///
    /// # Type Parameters
    /// - `D`: Deserializer borrowing the encoded input.
    ///
    /// # Parameters
    /// - `deserializer`: Input source for the jitter representation.
    ///
    /// # Returns
    /// The decoded jitter value, ready for enclosing policy validation.
    ///
    /// # Errors
    /// Rejects malformed or irrelevant fields, including a null numeric ratio.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = JitterStrategyDataRaw::deserialize(deserializer)?;
        match (raw.tag, raw.ratio) {
            (JitterStrategyTag::None, RatioField::Missing) => Ok(Self::None),
            (JitterStrategyTag::Full, RatioField::Missing) => Ok(Self::Full),
            (JitterStrategyTag::Bounded, RatioField::Present(ratio)) => Ok(Self::Bounded { ratio }),
            (JitterStrategyTag::Bounded, RatioField::Missing) => Err(D::Error::custom("bounded jitter requires ratio")),
            (_, RatioField::Present(_)) => Err(D::Error::custom("jitter ratio is only valid for bounded jitter")),
        }
    }
}

impl From<JitterStrategy> for JitterStrategyData {
    /// Converts runtime jitter to its stable wire representation.
    ///
    /// # Parameters
    /// - `strategy`: Runtime strategy to encode.
    ///
    /// # Returns
    /// The corresponding representation; enclosing policy validation enforces
    /// invariants.
    #[inline]
    fn from(strategy: JitterStrategy) -> Self {
        match strategy {
            JitterStrategy::None => Self::None,
            JitterStrategy::Full => Self::Full,
            JitterStrategy::Bounded { ratio } => Self::Bounded { ratio },
        }
    }
}

impl From<JitterStrategyData> for JitterStrategy {
    /// Converts wire jitter before enclosing policy validation checks its
    /// ratio.
    ///
    /// # Parameters
    /// - `data`: Decoded wire strategy to represent at runtime.
    ///
    /// # Returns
    /// The corresponding representation; enclosing policy validation enforces
    /// invariants.
    #[inline]
    fn from(data: JitterStrategyData) -> Self {
        match data {
            JitterStrategyData::None => Self::None,
            JitterStrategyData::Full => Self::Full,
            JitterStrategyData::Bounded { ratio } => Self::Bounded { ratio },
        }
    }
}
