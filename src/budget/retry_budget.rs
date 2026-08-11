// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Defines the reusable retry continuation-budget state machine.

use std::time::Duration;

use qubit_budget::DurationBudget;
use qubit_budget::ResourceBudget;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;

use super::RetryAttempt;
use super::RetryBudgetError;
use super::RetryBudgetExhausted;
use super::RetryBudgetSnapshot;
use crate::RetryLimits;

/// Internal diagnostic labels retained by primitive budget values.
#[derive(Debug, Clone)]
enum RetryResource {
    /// The finite count of admitted attempts.
    Attempts,
    /// The finite sum of operation durations.
    OperationElapsed,
}

/// The single source of truth for retry continuation limits.
///
/// `max_attempts`, `max_operation_elapsed`, and `max_total_elapsed` prevent
/// future attempts or retry sleeps. They never cancel an admitted attempt; a
/// completed successful attempt therefore wins even when it overran a limit.
/// Hard cancellation belongs to the executor's attempt and flow timeouts.
#[must_use]
pub struct RetryBudget<'a> {
    /// Clock used for operation duration and snapshot samples.
    clock: &'a dyn MonotonicClock,

    /// First sample of the whole retry flow.
    started_at: MonotonicInstant,

    /// Admitted attempts budget.
    attempts: ResourceBudget<RetryResource, u32>,

    /// Explicitly measured operation time budget.
    operation: Option<DurationBudget<RetryResource>>,

    /// Actual operation time, which can exceed the continuation allowance.
    operation_elapsed: Duration,

    /// Continuous end-to-end deadline.
    total_deadline: Option<MonotonicInstant>,

    /// Actual duration of the latest completed attempt.
    last_attempt_elapsed: Duration,
}

impl<'a> RetryBudget<'a> {
    /// Creates a retry budget from validated retry limits.
    ///
    /// The clock is sampled once for the initial snapshot and, when configured,
    /// for the total elapsed deadline. Returns [`RetryBudgetError::Clock`] if
    /// that deadline cannot be represented by the clock.
    pub fn new(
        clock: &'a dyn MonotonicClock,
        limits: RetryLimits,
    ) -> Result<Self, RetryBudgetError> {
        let started_at = clock.now();
        let total_deadline = limits
            .max_total_elapsed()
            .map(|duration| started_at.checked_add(duration))
            .transpose()
            .map_err(RetryBudgetError::Clock)?;
        Ok(Self {
            started_at,
            attempts: ResourceBudget::new(RetryResource::Attempts, limits.max_attempts().get()),
            operation: limits
                .max_operation_elapsed()
                .map(|duration| DurationBudget::new(RetryResource::OperationElapsed, duration)),
            operation_elapsed: Duration::ZERO,
            total_deadline,
            last_attempt_elapsed: Duration::ZERO,
            clock,
        })
    }

    /// Samples and returns the current retry budget state.
    pub fn snapshot(&self) -> RetryBudgetSnapshot {
        RetryBudgetSnapshot::new(
            self.attempts.used(),
            self.operation_elapsed,
            self.elapsed_since_started(),
            self.last_attempt_elapsed,
        )
    }

    /// Checks continuation limits and admits one new attempt.
    ///
    /// Returns the linear token required to finish that attempt, or the first
    /// exhausted limit in stable attempts, operation, total order. This method
    /// mutates only the attempt count when it succeeds.
    pub fn begin_attempt(&mut self) -> Result<RetryAttempt, RetryBudgetExhausted> {
        self.check_continuation()?;
        let number = self.attempts.used() + 1;
        let consumed = self.attempts.consume_available(1);
        debug_assert_eq!(consumed, 1, "checked budget must admit one attempt");
        Ok(RetryAttempt {
            number,
            started_at: self.clock.now(),
        })
    }

    /// Completes an admitted attempt and records its actual elapsed duration.
    ///
    /// The token is consumed exactly once. An overrun exhausts the operation
    /// allowance for future work but is retained exactly in the returned
    /// snapshot and never changes a completed attempt's outcome.
    pub fn finish_attempt(&mut self, attempt: RetryAttempt) -> RetryBudgetSnapshot {
        debug_assert_eq!(
            attempt.number,
            self.attempts.used(),
            "attempt must be finished in admission order",
        );
        let elapsed = self
            .clock
            .now()
            .duration_since(attempt.started_at)
            .expect("retry clock must stay monotonic");
        self.last_attempt_elapsed = elapsed;
        self.operation_elapsed = self.operation_elapsed.saturating_add(elapsed);
        if let Some(operation) = &mut self.operation {
            let _ = operation.consume_available(elapsed);
        }
        self.snapshot()
    }

    /// Checks whether the next retry action and its delay may continue.
    ///
    /// A delay that reaches the total deadline is rejected. The next call to
    /// [`Self::begin_attempt`] rechecks all limits after the delay and any
    /// observer work has elapsed.
    pub fn check_retry_after(&self, delay: Duration) -> Result<(), RetryBudgetExhausted> {
        self.check_continuation()?;
        let now = self.clock.now();
        if self
            .total_deadline
            .is_some_and(|deadline| match now.checked_add(delay) {
                Ok(end) => end >= deadline,
                Err(_) => true,
            })
        {
            return Err(RetryBudgetExhausted::TotalElapsed);
        }
        Ok(())
    }

    /// Applies the stable exhaustion priority to future continuation.
    fn check_continuation(&self) -> Result<(), RetryBudgetExhausted> {
        if self.attempts.remaining() == 0 {
            return Err(RetryBudgetExhausted::Attempts);
        }
        if self
            .operation
            .as_ref()
            .is_some_and(|budget| budget.remaining() == Duration::ZERO)
        {
            return Err(RetryBudgetExhausted::OperationElapsed);
        }
        if self
            .total_deadline
            .is_some_and(|deadline| self.clock.now() >= deadline)
        {
            return Err(RetryBudgetExhausted::TotalElapsed);
        }
        Ok(())
    }

    /// Samples total elapsed duration from the initial clock sample.
    fn elapsed_since_started(&self) -> Duration {
        self.clock
            .now()
            .duration_since(self.started_at)
            .expect("retry clock must stay monotonic")
    }
}
