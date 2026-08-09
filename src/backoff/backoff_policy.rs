// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Opaque backoff policy and validated constructors.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

use super::BackoffRequest;
use super::BackoffState;
use super::BackoffStep;
use super::backoff_delay_source::BackoffDelaySource;
use crate::RetryPolicyError;
use crate::RetryRandomSource;
use crate::random::ThreadRetryRandomSource;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BackoffStrategy {
    Immediate,
    Fixed {
        delay: Duration,
    },
    Uniform {
        min: Duration,
        max: Duration,
    },
    Exponential {
        initial: Duration,
        multiplier: f64,
        max: Duration,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum JitterStrategy {
    None,
    Full,
    Bounded { ratio: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RetryAfterStrategy {
    PreferHint,
    AtLeastBackoff,
    IgnoreHint,
}

/// Immutable delay strategy shared by retry and reconnect flows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        if min > max {
            return Err(RetryPolicyError::new(
                "backoff.uniform",
                "minimum delay must not exceed maximum delay",
            ));
        }
        Ok(Self {
            strategy: BackoffStrategy::Uniform { min, max },
            ..Self::immediate()
        })
    }

    /// Creates an exponential policy with a finite multiplier at least one.
    pub fn exponential(
        initial: Duration,
        multiplier: f64,
        max: Duration,
    ) -> Result<Self, RetryPolicyError> {
        if initial > max {
            return Err(RetryPolicyError::new(
                "backoff.exponential",
                "initial delay must not exceed maximum delay",
            ));
        }
        if !multiplier.is_finite() || multiplier < 1.0 {
            return Err(RetryPolicyError::new(
                "backoff.exponential.multiplier",
                "multiplier must be finite and at least 1.0",
            ));
        }
        Ok(Self {
            strategy: BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            },
            ..Self::immediate()
        })
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
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(RetryPolicyError::new(
                "backoff.jitter.ratio",
                "jitter ratio must be finite and within 0.0..=1.0",
            ));
        }
        self.jitter = JitterStrategy::Bounded { ratio };
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
    pub fn maximum_delay(&self) -> Option<Duration> {
        match &self.strategy {
            BackoffStrategy::Immediate => Some(Duration::ZERO),
            BackoffStrategy::Fixed { delay } => Some(*delay),
            BackoffStrategy::Uniform { max, .. }
            | BackoffStrategy::Exponential { max, .. } => Some(*max),
        }
    }

    /// Starts a state with the default thread-local random source.
    pub fn start(&self) -> BackoffState {
        self.start_with_random_source(Arc::new(ThreadRetryRandomSource))
    }

    /// Starts a state with a deterministic or custom random source.
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
        if let Some(explicit) = request.explicit_delay {
            return BackoffStep::new(
                retry_index,
                base_delay,
                explicit,
                BackoffDelaySource::Explicit,
            );
        }
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
        let effective_delay = match self.retry_after {
            RetryAfterStrategy::PreferHint => hinted_delay,
            RetryAfterStrategy::AtLeastBackoff => {
                policy_delay.max(hinted_delay)
            }
            RetryAfterStrategy::IgnoreHint => policy_delay,
        };
        let source = match self.retry_after {
            RetryAfterStrategy::PreferHint => BackoffDelaySource::Hint,
            RetryAfterStrategy::AtLeastBackoff => BackoffDelaySource::Merged,
            RetryAfterStrategy::IgnoreHint => BackoffDelaySource::Policy,
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
