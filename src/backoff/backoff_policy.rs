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

/// Immutable delay strategy shared by retry and reconnect flows.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_retry::BackoffPolicy;
/// use qubit_retry::BackoffRequest;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let policy = BackoffPolicy::exponential(Duration::from_millis(50), 2.0, Duration::from_secs(2))?
///         .with_full_jitter()
///         .use_retry_after_as_minimum()
///         .limit_delay(Duration::from_millis(500));
///     let mut state = policy.start();
///     let step = state.next(BackoffRequest::hint(Duration::from_secs(1)));
///     assert_eq!(step.effective_delay(), Duration::from_millis(500));
///     assert_eq!(step.retry_index(), 1);
///     state.reset();
///     assert_eq!(state.retry_index(), 0);
///     Ok(())
/// }
/// ```
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub struct BackoffPolicy {
    /// Validated base-delay calculation before hint or jitter processing.
    strategy: BackoffStrategy,
    /// Variation applied to policy delay and explicitly jitterable hints.
    jitter: JitterStrategy,
    /// Precedence of a caller hint relative to the policy delay.
    retry_after: RetryAfterStrategy,
    /// Optional cap applied after jitter and hint resolution.
    delay_limit: Option<Duration>,
}

impl BackoffPolicy {
    /// Creates immediate retries with no jitter.
    ///
    /// # Returns
    /// A zero-delay policy without jitter or final cap; hints act as a minimum.
    #[inline]
    pub fn immediate() -> Self {
        Self {
            strategy: BackoffStrategy::Immediate,
            jitter: JitterStrategy::None,
            retry_after: RetryAfterStrategy::AtLeastBackoff,
            delay_limit: None,
        }
    }

    /// Creates a fixed-delay policy.
    ///
    /// # Parameters
    /// - `delay`: Base delay, including zero for immediate retries.
    ///
    /// # Returns
    /// A fixed policy without jitter or final cap; hints act as a minimum.
    #[inline]
    pub fn fixed(delay: Duration) -> Self {
        Self {
            strategy: BackoffStrategy::Fixed { delay },
            ..Self::immediate()
        }
    }

    /// Creates a uniformly distributed base-delay policy.
    ///
    /// # Parameters
    /// - `min`: Inclusive lower base-delay bound.
    /// - `max`: Inclusive upper base-delay bound.
    ///
    /// # Returns
    /// A validated uniform policy, without jitter or final cap.
    ///
    /// # Errors
    /// Returns a policy error when `min > max`. Equal bounds are valid.
    #[inline]
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
    ///
    /// # Parameters
    /// - `initial`: Base delay at retry index one.
    /// - `multiplier`: Finite growth factor, at least one.
    /// - `max`: Inclusive base-delay cap, at least `initial`.
    ///
    /// # Returns
    /// A validated exponential policy; overflow saturates at `max`.
    ///
    /// # Errors
    /// Returns a policy error for reversed bounds or an invalid multiplier.
    #[inline]
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

    /// Starts a state with the default thread-local random source.
    ///
    /// # Returns
    /// Fresh state at retry index zero, owning a policy clone and thread RNG.
    #[must_use]
    #[inline(always)]
    pub fn start(&self) -> BackoffState {
        BackoffState::new_thread(self.clone())
    }

    /// Starts a state with a deterministic or custom random source.
    ///
    /// # Parameters
    /// - `random`: Shared source required to return finite in-range samples.
    ///
    /// # Returns
    /// Fresh state at index zero; the source is shared and the policy is
    /// cloned.
    #[must_use]
    #[inline(always)]
    pub fn start_with_random_source(
        &self,
        random: Arc<dyn RetryRandomSource>,
    ) -> BackoffState {
        BackoffState::new(self.clone(), random)
    }

    /// Returns the final resolved-delay cap, or `None` when it is unbounded.
    ///
    /// # Returns
    /// `Some(limit)` for an explicit final cap, or `None` for no final cap.
    #[inline(always)]
    #[must_use]
    pub fn delay_limit(&self) -> Option<Duration> {
        self.delay_limit
    }

    /// Returns the maximum base-policy delay, before jitter and server hints.
    /// This is not a bound on the final sleep; use [`Self::limit_delay`] for
    /// that.
    ///
    /// # Returns
    /// `Some(maximum)` for every current bounded base strategy. The optional
    /// return leaves room for a future strategy without a finite maximum.
    #[must_use]
    #[inline(always)]
    pub fn maximum_delay(&self) -> Option<Duration> {
        match &self.strategy {
            BackoffStrategy::Immediate => Some(Duration::ZERO),
            BackoffStrategy::Fixed { delay } => Some(*delay),
            BackoffStrategy::Uniform { max, .. }
            | BackoffStrategy::Exponential { max, .. } => Some(*max),
        }
    }

    /// Disables jitter.
    ///
    /// # Returns
    /// The owned policy with variation disabled, retaining hints and the final
    /// cap.
    #[inline(always)]
    pub fn without_jitter(mut self) -> Self {
        self.jitter = JitterStrategy::None;
        self
    }

    /// Applies full jitter to calculated policy delays.
    ///
    /// # Returns
    /// The owned policy sampling from zero through each eligible delay.
    #[inline(always)]
    pub fn with_full_jitter(mut self) -> Self {
        self.jitter = JitterStrategy::Full;
        self
    }

    /// Applies symmetric bounded jitter.
    ///
    /// # Parameters
    /// - `ratio`: Finite relative deviation in the inclusive range zero to one.
    ///
    /// # Returns
    /// The owned policy with symmetric multiplicative jitter.
    ///
    /// # Errors
    /// Returns a policy error if `ratio` is not finite or is outside `[0, 1]`.
    #[inline]
    pub fn with_bounded_jitter(
        mut self,
        ratio: f64,
    ) -> Result<Self, RetryPolicyError> {
        self.jitter = JitterStrategy::Bounded { ratio };
        self.validate()?;
        Ok(self)
    }

    /// Prefers a hint over the configured policy delay.
    ///
    /// # Returns
    /// The owned policy selecting an available hint before the final cap.
    #[inline(always)]
    pub fn prefer_retry_after(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::PreferHint;
        self
    }

    /// Uses a hint as the minimum delay.
    ///
    /// The final cap may still truncate this minimum. Do not set a smaller cap
    /// when the server minimum is mandatory.
    ///
    /// # Returns
    /// The owned policy taking the larger of the hint and jittered policy
    /// delay.
    #[inline(always)]
    pub fn use_retry_after_as_minimum(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::AtLeastBackoff;
        self
    }

    /// Ignores hints.
    ///
    /// # Returns
    /// The owned policy resolving delays without caller hints.
    #[inline(always)]
    pub fn ignore_retry_after(mut self) -> Self {
        self.retry_after = RetryAfterStrategy::IgnoreHint;
        self
    }

    /// Sets a final upper bound for every resolved delay, including jitter and
    /// hints. Zero produces immediate retries. By default no final bound is
    /// applied.
    ///
    /// This cap takes precedence over a minimum server hint; callers must
    /// decide whether shortening that minimum is acceptable.
    ///
    /// # Parameters
    /// - `limit`: Inclusive final-delay cap; zero is permitted.
    ///
    /// # Returns
    /// The owned policy with the cap applied after all other transformations.
    #[inline(always)]
    pub fn limit_delay(mut self, limit: Duration) -> Self {
        self.delay_limit = Some(limit);
        self
    }

    ///
    /// Selects one base delay without hint, jitter, or final-cap processing.
    ///
    /// # Parameters
    /// - `retry_index`: One-based retry index; exponential growth saturates.
    /// - `random`: Source used only by the uniform base strategy.
    ///
    /// # Returns
    /// A delay bounded by the configured base strategy.
    #[must_use]
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

    /// Resolves the selected delay and applies the final cap after all
    /// transformations.
    ///
    /// # Parameters
    /// - `base_delay`: Previously selected strategy delay.
    /// - `request`: Optional hint and permission to jitter it.
    /// - `retry_index`: One-based index recorded in the resulting step.
    /// - `random`: Shared source for any permitted jitter sampling.
    ///
    /// # Returns
    /// The selected source and base delay, with the final effective delay
    /// capped.
    #[inline]
    pub(crate) fn resolve(
        &self,
        base_delay: Duration,
        request: BackoffRequest,
        retry_index: u32,
        random: &dyn RetryRandomSource,
    ) -> BackoffStep {
        let step =
            self.resolve_uncapped(base_delay, request, retry_index, random);
        let delay = self.delay_limit.map_or(step.effective_delay(), |limit| {
            step.effective_delay().min(limit)
        });
        BackoffStep::new(retry_index, base_delay, delay, step.source())
    }

    /// Applies hint selection and jitter without an end-to-end delay cap.
    ///
    /// # Parameters
    /// - `base_delay`: Previously selected strategy delay.
    /// - `request`: Hint and permission to apply jitter to that hint.
    /// - `retry_index`: One-based index copied into the step.
    /// - `random`: Source for policy and, when permitted, hint jitter.
    ///
    /// # Returns
    /// A source-tagged step before any final cap is applied.
    fn resolve_uncapped(
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

    ///
    /// Varies one eligible delay according to the validated jitter strategy.
    ///
    /// # Parameters
    /// - `base`: Policy delay or a caller hint explicitly permitting jitter.
    /// - `random`: Finite in-range sampler.
    ///
    /// # Returns
    /// The unchanged, full-jittered, or bounded-jittered delay with saturation.
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
                if ratio == 0.0 {
                    return base;
                }
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
    ///
    /// # Returns
    /// Unit when all strategy bounds, multipliers and ratios are valid.
    ///
    /// # Errors
    /// Returns the offending policy field and reason for an invalid value.
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
        BackoffPolicyData::from(self).serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for BackoffPolicy {
    /// Deserializes and validates one backoff policy.
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
        let data = BackoffPolicyData::deserialize(deserializer)?;
        Self::try_from(data).map_err(Error::custom)
    }
}

#[cfg(feature = "serde")]
impl From<&BackoffPolicy> for BackoffPolicyData {
    /// Copies a runtime policy into its stable wire representation.
    ///
    /// # Parameters
    /// - `policy`: Validated runtime policy to encode.
    ///
    /// # Returns
    /// A stable wire DTO containing the same configuration.
    #[inline]
    fn from(policy: &BackoffPolicy) -> Self {
        Self {
            strategy: (&policy.strategy).into(),
            jitter: policy.jitter.into(),
            retry_after: policy.retry_after.into(),
            delay_limit: policy
                .delay_limit
                .map(crate::policy::internal::DurationData::from),
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<BackoffPolicyData> for BackoffPolicy {
    /// Invalid encoded duration or policy invariant.
    type Error = RetryPolicyError;

    /// Converts wire data and validates strategy and jitter invariants.
    ///
    /// # Parameters
    /// - `data`: Decoded wire configuration.
    ///
    /// # Returns
    /// A validated runtime policy.
    ///
    /// # Errors
    /// Rejects invalid durations, inverted ranges, nonfinite multipliers, and
    /// invalid jitter ratios.
    #[inline]
    fn try_from(data: BackoffPolicyData) -> Result<Self, Self::Error> {
        let policy = Self {
            strategy: data.strategy.try_into()?,
            jitter: data.jitter.into(),
            retry_after: data.retry_after.into(),
            delay_limit: data.delay_limit.map(TryInto::try_into).transpose()?,
        };
        policy.validate()?;
        Ok(policy)
    }
}

///
/// Calculates a capped exponential delay without integer exponent wrapping.
///
/// # Parameters
/// - `initial`: First retry delay.
/// - `multiplier`: Validated finite growth factor, at least one.
/// - `max`: Inclusive strategy cap.
/// - `retry_index`: One-based index, saturating index zero to the first retry.
///
/// # Returns
/// A base delay no greater than `max`, including when growth overflows.
#[must_use]
fn exponential_delay(
    initial: Duration,
    multiplier: f64,
    max: Duration,
    retry_index: u32,
) -> Duration {
    if retry_index <= 1 || initial.is_zero() || multiplier == 1.0 {
        return initial.min(max);
    }
    let exponent = f64::from(retry_index.saturating_sub(1));
    let seconds = initial.as_secs_f64() * multiplier.powf(exponent);
    if !seconds.is_finite() {
        return max;
    }
    Duration::try_from_secs_f64(seconds).unwrap_or(max).min(max)
}

///
/// Scales a duration and saturates unrepresentable values.
///
/// # Parameters
/// - `duration`: Nonnegative delay to scale.
/// - `factor`: Finite nonnegative factor supplied by validated jitter logic.
///
/// # Returns
/// Rounded duration, or `Duration::MAX` if the result cannot be represented.
#[inline]
#[must_use]
fn scale_duration(duration: Duration, factor: f64) -> Duration {
    let seconds = duration.as_secs_f64() * factor;
    if !seconds.is_finite() || seconds >= Duration::MAX.as_secs_f64() {
        Duration::MAX
    } else {
        Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX)
    }
}

///
/// Samples a duration interval with exact endpoints and a final range clamp.
///
/// # Parameters
/// - `min`: Inclusive lower bound.
/// - `max`: Inclusive upper bound, at least `min` for valid callers.
/// - `sample`: Finite normalized sample in `[0, 1]`.
///
/// # Returns
/// A duration within the closed interval; samples zero/one return exact
/// endpoints without a floating Duration round trip.
#[must_use]
fn interpolate(min: Duration, max: Duration, sample: f64) -> Duration {
    if min >= max {
        return min;
    }
    let ratio = sample.clamp(0.0, 1.0);
    if ratio == 0.0 {
        return min;
    }
    if ratio == 1.0 {
        return max;
    }
    let span = max.saturating_sub(min);
    min.saturating_add(scale_duration(span, ratio)).min(max)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::exponential_delay;

    /// Private sequence extremes are infeasible to reach by billions of next
    /// calls.
    #[test]
    fn test_exponential_delay_never_wraps_the_exponent() {
        for index in [i32::MAX as u32 + 1, i32::MAX as u32 + 2, u32::MAX] {
            assert_eq!(
                exponential_delay(
                    Duration::from_secs(1),
                    2.0,
                    Duration::from_secs(10),
                    index
                ),
                Duration::from_secs(10)
            );
        }
    }

    /// Zero initial delay remains zero even when the multiplier power
    /// overflows.
    #[test]
    fn test_exponential_delay_zero_initial_remains_zero() {
        assert_eq!(
            exponential_delay(
                Duration::ZERO,
                2.0,
                Duration::from_secs(10),
                2048
            ),
            Duration::ZERO
        );
    }
}
