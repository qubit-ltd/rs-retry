// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Opaque backoff policy and validated constructors.

use std::sync::Arc;
use std::time::Duration;

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

use super::BackoffRequest;
use super::BackoffState;
use super::BackoffStep;
use super::backoff_delay_source::BackoffDelaySource;
#[cfg(feature = "serde")]
use super::internal::BackoffPolicyData;
use super::internal::BackoffStrategy;
use super::internal::JitterStrategy;
use super::internal::RetryAfterStrategy;
use crate::RetryPolicyError;
use crate::RetryRandomSource;
use crate::random::ThreadRetryRandomSource;

/// Immutable delay strategy shared by retry and reconnect flows.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub struct BackoffPolicy {
    strategy: BackoffStrategy,
    jitter: JitterStrategy,
    retry_after: RetryAfterStrategy,
}

impl BackoffPolicy {
    /// Creates immediate retries with no jitter.
    pub fn immediate() -> Self {
        Self {
            strategy: BackoffStrategy::Immediate,
            jitter: JitterStrategy::None,
            retry_after: RetryAfterStrategy::AtLeastBackoff,
        }
    }

    /// Creates a fixed-delay policy.
    pub fn fixed(delay: Duration) -> Self {
        Self {
            strategy: BackoffStrategy::Fixed { delay },
            ..Self::immediate()
        }
    }

    /// Creates a uniformly distributed base-delay policy.
    pub fn uniform(
        min: Duration,
        max: Duration,
    ) -> Result<Self, RetryPolicyError> {
        let policy = Self {
            strategy: BackoffStrategy::Uniform { min, max },
            ..Self::immediate()
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Creates an exponential policy with a finite multiplier at least one.
    pub fn exponential(
        initial: Duration,
        multiplier: f64,
        max: Duration,
    ) -> Result<Self, RetryPolicyError> {
        let policy = Self {
            strategy: BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            },
            ..Self::immediate()
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Disables jitter.
    pub fn without_jitter(mut self) -> Self {
        self.jitter = JitterStrategy::None;
        self
    }

    /// Applies full jitter to calculated policy delays.
    pub fn with_full_jitter(mut self) -> Self {
        self.jitter = JitterStrategy::Full;
        self
    }

    /// Applies symmetric bounded jitter.
    pub fn with_bounded_jitter(
        mut self,
        ratio: f64,
    ) -> Result<Self, RetryPolicyError> {
        self.jitter = JitterStrategy::Bounded { ratio };
        self.validate()?;
        Ok(self)
    }

    /// Prefers a hint over the configured policy delay.
    pub fn prefer_retry_after(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::PreferHint;
        self
    }

    /// Uses a hint as the minimum delay.
    pub fn use_retry_after_as_minimum(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::AtLeastBackoff;
        self
    }

    /// Ignores hints.
    pub fn ignore_retry_after(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::IgnoreHint;
        self
    }

    /// Returns the configured maximum policy delay, when one exists.
    #[must_use]
    pub fn maximum_delay(&self) -> Option<Duration> {
        match &self.strategy {
            BackoffStrategy::Immediate => Some(Duration::ZERO),
            BackoffStrategy::Fixed { delay } => Some(*delay),
            BackoffStrategy::Uniform { max, .. }
            | BackoffStrategy::Exponential { max, .. } => Some(*max),
        }
    }

    /// Starts a state with the default thread-local random source.
    #[must_use]
    pub fn start(&self) -> BackoffState {
        self.start_with_random_source(Arc::new(ThreadRetryRandomSource))
    }

    /// Starts a state with a deterministic or custom random source.
    #[must_use]
    pub fn start_with_random_source(
        &self,
        random: Arc<dyn RetryRandomSource>,
    ) -> BackoffState {
        BackoffState::new(self.clone(), random)
    }

    pub(crate) fn base_delay(
        &self,
        retry_index: u32,
        random: &dyn RetryRandomSource,
    ) -> Duration {
        match &self.strategy {
            BackoffStrategy::Immediate => Duration::ZERO,
            BackoffStrategy::Fixed { delay } => *delay,
            BackoffStrategy::Uniform { min, max } => {
                interpolate(*min, *max, random.random_f64_inclusive(0.0, 1.0))
            }
            BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            } => exponential_delay(*initial, *multiplier, *max, retry_index),
        }
    }

    pub(crate) fn resolve(
        &self,
        base_delay: Duration,
        request: BackoffRequest,
        retry_index: u32,
        random: &dyn RetryRandomSource,
    ) -> BackoffStep {
        let Some(hint) = request.hint else {
            return BackoffStep::new(
                retry_index,
                base_delay,
                self.apply_jitter(base_delay, random),
                BackoffDelaySource::Policy,
            );
        };
        if self.retry_after == RetryAfterStrategy::IgnoreHint {
            return BackoffStep::new(
                retry_index,
                base_delay,
                self.apply_jitter(base_delay, random),
                BackoffDelaySource::Policy,
            );
        }
        let policy_delay = self.apply_jitter(base_delay, random);
        let hinted_delay = if request.jitter_hint {
            self.apply_jitter(hint, random)
        } else {
            hint
        };
        let (effective_delay, source) =
            if self.retry_after == RetryAfterStrategy::PreferHint {
                (hinted_delay, BackoffDelaySource::Hint)
            } else {
                debug_assert_eq!(
                    self.retry_after,
                    RetryAfterStrategy::AtLeastBackoff
                );
                (policy_delay.max(hinted_delay), BackoffDelaySource::Merged)
            };
        BackoffStep::new(retry_index, base_delay, effective_delay, source)
    }

    fn apply_jitter(
        &self,
        base: Duration,
        random: &dyn RetryRandomSource,
    ) -> Duration {
        match self.jitter {
            JitterStrategy::None => base,
            JitterStrategy::Full => interpolate(
                Duration::ZERO,
                base,
                random.random_f64_inclusive(0.0, 1.0),
            ),
            JitterStrategy::Bounded { ratio } => {
                let low = (1.0 - ratio).max(0.0);
                let high = 1.0 + ratio;
                interpolate(
                    scale_duration(base, low),
                    scale_duration(base, high),
                    random.random_f64_inclusive(0.0, 1.0),
                )
            }
        }
    }

    /// Validates all invariants required by a backoff policy.
    fn validate(&self) -> Result<(), RetryPolicyError> {
        match &self.strategy {
            BackoffStrategy::Immediate | BackoffStrategy::Fixed { .. } => {}
            BackoffStrategy::Uniform { min, max } if min <= max => {}
            BackoffStrategy::Uniform { .. } => {
                return Err(RetryPolicyError::new(
                    "backoff.uniform",
                    "minimum delay must not exceed maximum delay",
                ));
            }
            BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            } => {
                if initial > max {
                    return Err(RetryPolicyError::new(
                        "backoff.exponential",
                        "initial delay must not exceed maximum delay",
                    ));
                }
                if !multiplier.is_finite() || *multiplier < 1.0 {
                    return Err(RetryPolicyError::new(
                        "backoff.exponential.multiplier",
                        "multiplier must be finite and at least 1.0",
                    ));
                }
            }
        }
        if let JitterStrategy::Bounded { ratio } = self.jitter
            && (!ratio.is_finite() || !(0.0..=1.0).contains(&ratio))
        {
            return Err(RetryPolicyError::new(
                "backoff.jitter.ratio",
                "jitter ratio must be finite and within 0.0..=1.0",
            ));
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl Serialize for BackoffPolicy {
    /// Serializes a policy through the stable private wire DTO.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BackoffPolicyData::from(self).serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for BackoffPolicy {
    /// Deserializes and validates one backoff policy.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = BackoffPolicyData::deserialize(deserializer)?;
        Self::try_from(data).map_err(Error::custom)
    }
}

#[cfg(feature = "serde")]
impl From<&BackoffPolicy> for BackoffPolicyData {
    /// Copies a runtime policy into its stable wire representation.
    fn from(policy: &BackoffPolicy) -> Self {
        Self {
            strategy: (&policy.strategy).into(),
            jitter: policy.jitter.into(),
            retry_after: policy.retry_after.into(),
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<BackoffPolicyData> for BackoffPolicy {
    type Error = RetryPolicyError;

    /// Converts wire data and validates strategy and jitter invariants.
    fn try_from(data: BackoffPolicyData) -> Result<Self, Self::Error> {
        let policy = Self {
            strategy: data.strategy.try_into()?,
            jitter: data.jitter.into(),
            retry_after: data.retry_after.into(),
        };
        policy.validate()?;
        Ok(policy)
    }
}

fn exponential_delay(
    initial: Duration,
    multiplier: f64,
    max: Duration,
    retry_index: u32,
) -> Duration {
    let exponent = retry_index.saturating_sub(1) as i32;
    let seconds = initial.as_secs_f64() * multiplier.powi(exponent);
    if !seconds.is_finite() {
        return max;
    }
    Duration::try_from_secs_f64(seconds).unwrap_or(max).min(max)
}

fn scale_duration(duration: Duration, factor: f64) -> Duration {
    let seconds = duration.as_secs_f64() * factor;
    if !seconds.is_finite() || seconds >= Duration::MAX.as_secs_f64() {
        Duration::MAX
    } else {
        Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX)
    }
}

fn interpolate(min: Duration, max: Duration, sample: f64) -> Duration {
    if min >= max {
        return min;
    }
    let ratio = sample.clamp(0.0, 1.0);
    let span = max.saturating_sub(min);
    min.saturating_add(scale_duration(span, ratio))
}
