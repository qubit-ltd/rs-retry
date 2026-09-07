// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared continuation accounting driven by explicit monotonic samples.

use std::time::Duration;

use qubit_budget::ResourceBudget;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;

use super::RetryResource;
use crate::RetryAdmissionLimits;
use crate::RetryBudgetSnapshot;
use crate::RetryLimitKind;

/// The single accounting state used by public budgets and retry facades.
pub(crate) struct RetryBudgetState {
    /// Validated admission limits.
    limits: RetryAdmissionLimits,
    /// Initial sample for whole-flow elapsed time.
    started_at: MonotonicInstant,
    /// Latest committed sample; invalid samples never change accounting.
    sampled_at: MonotonicInstant,
    /// Number of admitted attempts.
    attempts: ResourceBudget<RetryResource, u32>,
    /// Accumulated completed operation duration.
    operation_elapsed: Duration,
    /// Most recently completed operation duration.
    last_attempt_elapsed: Duration,
    /// Start of the single active attempt, when present.
    attempt_started_at: Option<MonotonicInstant>,
}

impl RetryBudgetState {
    /// Starts accounting at one coherent sample without constructing a hard
    /// deadline.
    ///
    /// # Parameters
    /// - `started_at`: Valid initial clock-domain sample supplied by the
    ///   caller.
    /// - `limits`: Validated continuation limits.
    ///
    /// # Returns
    /// Empty accounting whose latest sample equals its start.
    #[inline]
    #[must_use = "use the prepared value or inspect the result"]
    pub(crate) fn new(started_at: MonotonicInstant, limits: RetryAdmissionLimits) -> Self {
        Self {
            limits,
            started_at,
            sampled_at: started_at,
            attempts: ResourceBudget::new(RetryResource::Attempts, limits.max_attempts().get()),
            operation_elapsed: Duration::ZERO,
            last_attempt_elapsed: Duration::ZERO,
            attempt_started_at: None,
        }
    }

    /// Returns the initial sample used by facade hard-flow deadlines.
    ///
    /// # Returns
    /// The original sample, used to anchor absolute flow timeouts.
    #[inline(always)]
    #[must_use = "use the flow start instant"]
    pub(crate) fn started_at(&self) -> MonotonicInstant {
        self.started_at
    }

    /// Returns whether an operation is currently admitted and unfinished.
    ///
    /// # Returns
    /// True exactly while an admitted operation has not been finished.
    #[inline(always)]
    #[must_use]
    pub(crate) fn has_active_attempt(&self) -> bool {
        self.attempt_started_at.is_some()
    }

    /// Returns the number of committed operations.
    ///
    /// # Returns
    /// Committed admissions, never exceeding the validated maximum.
    #[inline(always)]
    #[must_use]
    pub(crate) fn attempts(&self) -> u32 {
        self.attempts.used()
    }

    /// Returns accounting at the latest validated sample.
    ///
    /// # Returns
    /// Coherent accounting at the last committed sample.
    ///
    /// # Panics
    /// Panics if an internal mutation violated committed clock coherence.
    #[inline]
    #[must_use = "inspect the budget snapshot"]
    pub(crate) fn snapshot(&self) -> RetryBudgetSnapshot {
        self.snapshot_at(self.sampled_at)
            .expect("committed clock sample must remain coherent")
    }

    /// Samples an observation without mutation, rejecting a clock domain change
    /// or regression.
    ///
    /// # Parameters
    /// - `now`: Sample in the original domain, no earlier than the last commit.
    ///
    /// # Returns
    /// Updated total elapsed with unchanged completed-operation accounting.
    ///
    /// # Errors
    /// Returns a domain/regression error without mutating state.
    #[inline]
    #[must_use = "handle the budget snapshot result"]
    pub(crate) fn snapshot_at(&self, now: MonotonicInstant) -> Result<RetryBudgetSnapshot, TimeError> {
        let _ = now.duration_since(self.sampled_at)?;
        Ok(RetryBudgetSnapshot::new(
            self.attempts(),
            self.operation_elapsed,
            now.duration_since(self.started_at)?,
            self.last_attempt_elapsed,
        ))
    }

    /// Returns the first exhausted continuation limit, including a proposed
    /// delay.
    ///
    /// # Parameters
    /// - `delay`: Proposed delay, added with saturation to total elapsed time.
    ///
    /// # Returns
    /// `Some(kind)` for the first exhausted limit (attempts, operation, total),
    /// or `None` while all continuation limits permit another operation.
    #[must_use]
    pub(crate) fn retry_limit(&self, delay: Duration) -> Option<RetryLimitKind> {
        if self.attempts.remaining() == 0 {
            return Some(RetryLimitKind::Attempts);
        }
        if self
            .limits
            .operation_time_budget()
            .is_some_and(|limit| self.operation_elapsed >= limit)
        {
            return Some(RetryLimitKind::OperationElapsed);
        }
        if self
            .limits
            .total_time_budget()
            .is_some_and(|limit| self.snapshot().total_elapsed().saturating_add(delay) >= limit)
        {
            return Some(RetryLimitKind::TotalElapsed);
        }
        None
    }

    /// Validates and commits a sample; errors preserve all previous accounting.
    ///
    /// # Parameters
    /// - `now`: Candidate latest sample in this accounting domain.
    ///
    /// # Returns
    /// Unit after committing only a validated sample.
    ///
    /// # Errors
    /// Returns the clock validation error and retains previous accounting.
    #[inline]
    pub(crate) fn refresh(&mut self, now: MonotonicInstant) -> Result<(), TimeError> {
        let _ = self.snapshot_at(now)?;
        self.sampled_at = now;
        Ok(())
    }

    /// Commits an attempt after the caller checked limits and validated this
    /// sample.
    ///
    /// # Parameters
    /// - `now`: Validated sample for an eligible admission.
    ///
    /// # Panics
    /// Debug assertions fail if an operation is already active or attempt
    /// capacity is exhausted; callers must check eligibility before commitment.
    #[inline]
    pub(crate) fn begin_attempt(&mut self, now: MonotonicInstant) {
        debug_assert!(!self.has_active_attempt());
        let consumed = self.attempts.consume_available(1);
        debug_assert_eq!(consumed, 1);
        self.sampled_at = now;
        self.attempt_started_at = Some(now);
    }

    /// Completes the active attempt; invalid clock samples preserve its
    /// accounting.
    ///
    /// # Parameters
    /// - `now`: Completion sample in the same monotonic domain.
    ///
    /// # Returns
    /// Unit after adding the completed duration and clearing active state.
    ///
    /// # Errors
    /// Returns a clock error without changing accounting.
    ///
    /// # Panics
    /// Panics when called without a committed active attempt.
    pub(crate) fn finish_attempt(&mut self, now: MonotonicInstant) -> Result<(), TimeError> {
        let started_at = self
            .attempt_started_at
            .expect("an admitted attempt must be active before completion");
        let _ = self.snapshot_at(now)?;
        let elapsed = now.duration_since(started_at)?;
        self.sampled_at = now;
        self.last_attempt_elapsed = elapsed;
        self.operation_elapsed = self.operation_elapsed.saturating_add(elapsed);
        self.attempt_started_at = None;
        Ok(())
    }
}
