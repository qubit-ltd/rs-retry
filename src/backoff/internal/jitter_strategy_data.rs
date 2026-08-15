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

/// Deserialization-only jitter tag used to reject irrelevant variant fields.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JitterStrategyTag {
    /// Do not vary the selected delay.
    None,
    /// Sample from zero through the selected delay.
    Full,
    /// Apply a symmetric multiplicative range.
    Bounded,
}

/// Presence-aware jitter ratio field used to distinguish absent from `null`.
#[derive(Clone, Copy, Default)]
enum RatioField {
    /// The ratio field was absent.
    #[default]
    Missing,
    /// The ratio field was present with a numeric value.
    Present(f64),
}

/// Deserializes a present jitter ratio while rejecting JSON `null`.
fn deserialize_ratio<'de, D>(deserializer: D) -> Result<RatioField, D::Error>
where
    D: Deserializer<'de>,
{
    f64::deserialize(deserializer).map(RatioField::Present)
}

/// Deny-unknown-fields DTO used while selecting a jitter variant.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JitterStrategyDataRaw {
    /// Jitter variant discriminant.
    #[serde(rename = "type")]
    tag: JitterStrategyTag,
    /// Relative deviation accepted only by the bounded variant.
    #[serde(default, deserialize_with = "deserialize_ratio")]
    ratio: RatioField,
}

impl<'de> Deserialize<'de> for JitterStrategyData {
    /// Deserializes one jitter strategy and rejects irrelevant ratio fields.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = JitterStrategyDataRaw::deserialize(deserializer)?;
        match (raw.tag, raw.ratio) {
            (JitterStrategyTag::None, RatioField::Missing) => Ok(Self::None),
            (JitterStrategyTag::Full, RatioField::Missing) => Ok(Self::Full),
            (JitterStrategyTag::Bounded, RatioField::Present(ratio)) => {
                Ok(Self::Bounded { ratio })
            }
            (JitterStrategyTag::Bounded, RatioField::Missing) => {
                Err(D::Error::custom("bounded jitter requires ratio"))
            }
            (_, RatioField::Present(_)) => Err(D::Error::custom(
                "jitter ratio is only valid for bounded jitter",
            )),
        }
    }
}

impl From<JitterStrategy> for JitterStrategyData {
    /// Converts runtime jitter to its stable wire representation.
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
    fn from(data: JitterStrategyData) -> Self {
        match data {
            JitterStrategyData::None => Self::None,
            JitterStrategyData::Full => Self::Full,
            JitterStrategyData::Bounded { ratio } => Self::Bounded { ratio },
        }
    }
}
