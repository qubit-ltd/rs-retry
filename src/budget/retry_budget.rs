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
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_clock::ManualMonotonicClock;
/// use qubit_retry::RetryBudget;
/// use qubit_retry::RetryPolicy;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let clock = ManualMonotonicClock::new_shared();
///     let policy = RetryPolicy::builder().max_attempts(2).build()?;
///     let mut budget = RetryBudget::new(clock.as_ref(), *policy.limits())?;
///     let attempt = budget.begin_attempt()?;
///     clock.advance(Duration::from_secs(2))?;
///     let snapshot = budget.finish_attempt(attempt)?;
///     assert_eq!(snapshot.attempts(), 1);
///     assert_eq!(snapshot.operation_elapsed(), Duration::from_secs(2));
///     budget.check_retry_after(Duration::from_secs(1))?;
///     clock.advance(Duration::from_secs(1))?;
///     assert_eq!(budget.snapshot()?.total_elapsed(), Duration::from_secs(3));
///     Ok(())
/// }
/// ```
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
    ///
    /// # Parameters
    /// - `clock`: Borrowed monotonic source retained for the budget lifetime.
    /// - `limits`: Validated continuation limits, including the initial
    ///   operation.
    ///
    /// # Returns
    /// A fresh budget with a unique token identity and no active operation.
    ///
    /// # Errors
    /// Returns `Clock` when the initial sample does not belong to `clock`.
    #[inline]
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
    ///
    /// # Returns
    /// A current total-time observation and committed operation accounting.
    ///
    /// # Errors
    /// Returns `Clock` for a domain mismatch or sample regression.
    #[inline(always)]
    #[must_use = "handle the budget snapshot result"]
    pub fn snapshot(&self) -> Result<RetryBudgetSnapshot, RetryBudgetError> {
        Ok(self.state.snapshot_at(self.clock.now())?)
    }

    /// Admits one operation, or returns overlap, clock, or continuation
    /// exhaustion. Failed admissions never consume an attempt.
    ///
    /// # Returns
    /// A linear token for the newly committed attempt. Dropping it does not
    /// finish the active operation.
    ///
    /// # Errors
    /// Returns `AttemptInProgress`, `Clock`, or `Exhausted` without increasing
    /// the attempt count when admission fails.
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
    ///
    /// # Parameters
    /// - `attempt`: Token owned by this budget and consumed exactly once.
    ///
    /// # Returns
    /// The completed operation duration and coherent cumulative accounting.
    ///
    /// # Errors
    /// Returns `InvalidAttempt` for a foreign/inactive token, or `Clock` for
    /// invalid completion timing. The consumed token cannot be retried.
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
    ///
    /// # Parameters
    /// - `delay`: Proposed waiting time before the next admission.
    ///
    /// # Returns
    /// Unit when continuation is currently allowed, without sleeping or
    /// admitting work. Actual admission must recheck after waiting.
    ///
    /// # Errors
    /// Returns `AttemptInProgress`, `Clock`, or the first `Exhausted` limit.
    pub fn check_retry_after(&mut self, delay: Duration) -> Result<(), RetryBudgetError> {
        if self.state.has_active_attempt() {
            return Err(RetryBudgetError::AttemptInProgress);
        }
        self.state.refresh(self.clock.now())?;
        self.check_limit(delay)
    }

    /// Converts the shared continuation decision to the public budget error.
    ///
    /// # Parameters
    /// - `delay`: Proposed delay included only in total elapsed continuation.
    ///
    /// # Returns
    /// Unit if current attempts and elapsed budgets permit continuation.
    ///
    /// # Errors
    /// Returns `Exhausted` with attempt, operation, then total limit priority.
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
