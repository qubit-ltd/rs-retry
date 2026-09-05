// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reusable sequential retry continuation budgets.

use std::sync::Arc;
use std::time::Duration;

use qubit_clock::MonotonicClock;

use super::RetryAttempt;
use super::RetryBudgetError;
use super::RetryBudgetExhausted;
use super::RetryBudgetSnapshot;
use super::RetryBudgetState;
use crate::RetryLimitKind;
use crate::RetryLimits;

/// Sequential admission and elapsed-time accounting shared with retry facades.
///
/// Continuation limits never cancel admitted work. Finish every admitted token
/// before admitting another operation. Tokens are bound to this budget even
/// when another budget shares its clock and attempt ordinal. Dropping a token
/// leaves the budget closed to further admissions; start a new budget to
/// abandon that flow. Clock failures return errors rather than panicking.
#[must_use]
pub struct RetryBudget<'a> {
    /// Clock sampled once by each public operation.
    clock: &'a dyn MonotonicClock,
    /// Shared accounting and continuation checks.
    state: RetryBudgetState,
    /// Unique identity retained by this budget and its linear tokens.
    owner: Arc<()>,
}

impl<'a> RetryBudget<'a> {
    /// Starts a budget at one sample, rejecting a sample from the wrong clock
    /// domain. Soft elapsed limits do not require a representable absolute
    /// deadline.
    pub fn new(clock: &'a dyn MonotonicClock, limits: RetryLimits) -> Result<Self, RetryBudgetError> {
        let now = clock.now();
        now.validate_domain(clock.domain())?;
        Ok(Self {
            clock,
            state: RetryBudgetState::new(now, limits),
            owner: Arc::new(()),
        })
    }

    /// Returns a current observation or a clock error without changing
    /// accounting.
    pub fn snapshot(&self) -> Result<RetryBudgetSnapshot, RetryBudgetError> {
        Ok(self.state.snapshot_at(self.clock.now())?)
    }

    /// Admits one operation, or returns overlap, clock, or continuation
    /// exhaustion. Failed admissions never consume an attempt.
    pub fn begin_attempt(&mut self) -> Result<RetryAttempt, RetryBudgetError> {
        if self.state.has_active_attempt() {
            return Err(RetryBudgetError::AttemptInProgress);
        }
        let now = self.clock.now();
        self.state.refresh(now)?;
        self.check_limit(Duration::ZERO)?;
        self.state.begin_attempt(now);
        Ok(RetryAttempt {
            number: self.state.attempts(),
            owner: Arc::clone(&self.owner),
        })
    }

    /// Consumes a token and completes its operation, returning actual elapsed
    /// time. Foreign tokens and invalid clock samples leave accounting
    /// unchanged. A clock failure consumes the token and closes this flow
    /// to further admission.
    pub fn finish_attempt(&mut self, attempt: RetryAttempt) -> Result<RetryBudgetSnapshot, RetryBudgetError> {
        if !Arc::ptr_eq(&self.owner, &attempt.owner)
            || !self.state.has_active_attempt()
            || attempt.number != self.state.attempts()
        {
            return Err(RetryBudgetError::InvalidAttempt);
        }
        self.state.finish_attempt(self.clock.now())?;
        Ok(self.state.snapshot())
    }

    /// Checks a proposed delay, rejecting overlap, clock errors or exhausted
    /// limits. The next admission rechecks limits after the actual sleep.
    pub fn check_retry_after(&mut self, delay: Duration) -> Result<(), RetryBudgetError> {
        if self.state.has_active_attempt() {
            return Err(RetryBudgetError::AttemptInProgress);
        }
        self.state.refresh(self.clock.now())?;
        self.check_limit(delay)
    }

    /// Converts the shared continuation decision to the public budget error.
    fn check_limit(&self, delay: Duration) -> Result<(), RetryBudgetError> {
        match self.state.retry_limit(delay) {
            None => Ok(()),
            Some(limit) => Err(RetryBudgetError::Exhausted(match limit {
                RetryLimitKind::Attempts => RetryBudgetExhausted::Attempts,
                RetryLimitKind::OperationElapsed => RetryBudgetExhausted::OperationElapsed,
                RetryLimitKind::TotalElapsed => RetryBudgetExhausted::TotalElapsed,
            })),
        }
    }
}
