// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Strict input representation of a jitter strategy.

use serde::Deserialize;
use serde::Deserializer;

use super::jitter_strategy_tag::JitterStrategyTag;
use super::ratio_field::RatioField;

/// Deny-unknown-fields DTO used while selecting a jitter variant.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct JitterStrategyDataRaw {
    /// Jitter variant discriminant.
    #[serde(rename = "type")]
    pub(super) tag: JitterStrategyTag,
    /// Relative deviation accepted only by the bounded variant.
    #[serde(default, deserialize_with = "deserialize_ratio")]
    pub(super) ratio: RatioField,
}

/// Deserializes a present jitter ratio while rejecting JSON `null`.
///
/// # Type Parameters
/// - `D`: Deserializer borrowing the encoded input.
///
/// # Parameters
/// - `deserializer`: Input source for the jitter representation.
///
/// # Returns
/// A present numeric ratio; the enclosing policy validates its range.
///
/// # Errors
/// Returns a deserializer error for a nonnumeric value, including null.
#[inline(always)]
fn deserialize_ratio<'de, D>(deserializer: D) -> Result<RatioField, D::Error>
where
    D: Deserializer<'de>,
{
    f64::deserialize(deserializer).map(RatioField::Present)
}
