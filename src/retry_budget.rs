//! Shared attempt, active-duration, and end-to-end retry accounting.

use std::time::Duration;

use qubit_budget::BudgetError;
use qubit_budget::ResourceBudget;
use qubit_budget::ResourceLimit;
use qubit_clock::DurationBudget;
use qubit_clock::DurationBudgetError;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeBudget;
use qubit_clock::TimeBudgetError;

use crate::RetryLimits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryResource {
    Attempts,
}

/// Structured retry budget failures. Failed attempt admission does not charge
/// an attempt; a successfully admitted attempt remains charged forever.
#[derive(Debug)]
pub enum RetryBudgetExceeded {
    /// The attempt count is exhausted.
    Attempts(BudgetError<RetryResource, u32>),
    /// Active operation duration is exhausted; observed duration is retained.
    Operation(DurationBudgetError),
    /// End-to-end deadline or clock operation failed.
    Total(TimeBudgetError),
}

/// Combines monotonic attempt, active-operation, and total-flow budgets.
///
/// State transitions are admission `Open -> Open` with a charged attempt and
/// terminal caller-controlled stop when any component rejects. Backoff and
/// waiting affect only the total [`TimeBudget`]; callers explicitly charge the
/// measured operation interval in [`Self::finish_attempt`].
pub struct RetryBudget<C> {
    attempts: ResourceBudget<RetryResource, u32>,
    operation: DurationBudget,
    total: TimeBudget<C>,
}

impl<C: MonotonicClock> RetryBudget<C> {
    /// Creates a budget from validated retry limits.
    pub fn new(clock: C, limits: RetryLimits) -> Result<Self, TimeBudgetError> {
        let total = match limits.max_total_elapsed() {
            Some(duration) => TimeBudget::for_duration(clock, duration)?,
            None => TimeBudget::unlimited(clock),
        };
        let attempts = ResourceBudget::new(ResourceLimit::bounded(
            RetryResource::Attempts,
            limits.max_attempts().get(),
        ));
        let operation = limits
            .max_operation_elapsed()
            .map(DurationBudget::bounded)
            .unwrap_or_else(DurationBudget::unlimited);
        Ok(Self {
            attempts,
            operation,
            total,
        })
    }

    /// Admits and charges one attempt, returning its start instant.
    pub fn try_begin_attempt(
        &mut self,
    ) -> Result<MonotonicInstant, RetryBudgetExceeded> {
        self.total.check().map_err(RetryBudgetExceeded::Total)?;
        self.attempts
            .try_charge(1)
            .map_err(RetryBudgetExceeded::Attempts)?;
        Ok(self.total.sample())
    }

    /// Measures and charges one admitted attempt's active duration.
    pub fn finish_attempt(
        &mut self,
        started_at: MonotonicInstant,
    ) -> Result<Duration, RetryBudgetExceeded> {
        let elapsed = self
            .total
            .measure_since(started_at)
            .map_err(RetryBudgetExceeded::Total)?;
        self.operation
            .try_charge(elapsed)
            .map_err(RetryBudgetExceeded::Operation)?;
        Ok(elapsed)
    }

    /// Returns admitted attempts.
    pub fn attempts(&self) -> u32 {
        self.attempts.charged()
    }
    /// Returns accepted active operation duration.
    pub fn operation_elapsed(&self) -> Duration {
        self.operation.charged()
    }
    /// Returns end-to-end elapsed duration.
    pub fn total_elapsed(&self) -> Result<Duration, TimeBudgetError> {
        self.total.elapsed()
    }
    /// Returns the underlying total deadline budget.
    pub fn total(&self) -> &TimeBudget<C> {
        &self.total
    }
}
