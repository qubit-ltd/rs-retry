//! Internal composition of the finite budgets used by retry executors.
// qubit-style: allow multiple-public-types

use std::sync::Arc;
use std::time::Duration;

use qubit_budget::DurationBudget;
use qubit_budget::DurationBudgetError;
use qubit_budget::ResourceBudget;
use qubit_budget::ResourceBudgetError;
use qubit_budget::ResourceLimit;
use qubit_budget::TimeBudget;
use qubit_budget::TimeBudgetError;
use qubit_clock::ClockDomain;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::Timer;

use crate::RetryLimits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryResource {
    Attempts,
    OperationDuration,
    TotalElapsed,
}

pub(crate) struct ClockRef<'a>(pub(crate) &'a dyn MonotonicClock);

impl MonotonicClock for ClockRef<'_> {
    fn domain(&self) -> ClockDomain {
        self.0.domain()
    }

    fn now(&self) -> MonotonicInstant {
        self.0.now()
    }

    fn new_timer(&self) -> Arc<dyn Timer> {
        self.0.new_timer()
    }
}

#[derive(Debug)]
pub(crate) enum RetryBudgetExceeded {
    Attempts(ResourceBudgetError<RetryResource>),
    Operation(DurationBudgetError<RetryResource>),
    Total(TimeBudgetError<RetryResource>),
}

impl RetryBudgetExceeded {
    pub(crate) fn reason(&self) -> crate::RetryErrorReason {
        match self {
            Self::Attempts(error) => {
                let _ = error.requested();
                crate::RetryErrorReason::AttemptsExhausted
            }
            Self::Operation(error) => {
                let _ = error.requested();
                crate::RetryErrorReason::OperationBudgetExhausted
            }
            Self::Total(error) => {
                let _ = error.resource();
                crate::RetryErrorReason::TotalBudgetExhausted
            }
        }
    }
}

pub(crate) struct RetryBudget<C> {
    attempts: ResourceBudget<RetryResource>,
    operation: Option<DurationBudget<RetryResource>>,
    total: Option<TimeBudget<RetryResource, C>>,
}

impl<C: MonotonicClock> RetryBudget<C> {
    pub(crate) fn new(
        clock: C,
        limits: RetryLimits,
    ) -> Result<Self, TimeBudgetError<RetryResource>> {
        let total = limits
            .max_total_elapsed()
            .map(|duration| {
                TimeBudget::for_duration(
                    RetryResource::TotalElapsed,
                    clock,
                    duration,
                )
            })
            .transpose()?;
        let attempts = ResourceBudget::new(
            RetryResource::Attempts,
            ResourceLimit::new(u64::from(limits.max_attempts().get())),
        );
        let operation = limits.max_operation_elapsed().map(|duration| {
            DurationBudget::new(RetryResource::OperationDuration, duration)
        });
        Ok(Self {
            attempts,
            operation,
            total,
        })
    }

    pub(crate) fn check_before_attempt(&self) -> Result<(), RetryBudgetExceeded>
    where
        C: MonotonicClock,
    {
        if let Some(total) = &self.total {
            total.check().map_err(RetryBudgetExceeded::Total)?;
        }
        if let Some(operation) = &self.operation
            && operation.remaining() == Duration::ZERO
        {
            return Err(RetryBudgetExceeded::Operation(
                operation
                    .check_available(Duration::from_nanos(1))
                    .expect_err(
                        "zero remaining duration must reject a request",
                    ),
            ));
        }
        Ok(())
    }

    pub(crate) fn try_begin_attempt(
        &mut self,
    ) -> Result<(), RetryBudgetExceeded> {
        self.check_before_attempt()?;
        self.attempts
            .try_consume(1)
            .map_err(RetryBudgetExceeded::Attempts)?;
        Ok(())
    }

    pub(crate) fn finish_attempt(
        &mut self,
        elapsed: Duration,
    ) -> Result<(), RetryBudgetExceeded> {
        if let Some(operation) = &mut self.operation {
            operation
                .try_consume(elapsed)
                .map_err(RetryBudgetExceeded::Operation)?;
        }
        if let Some(total) = &self.total {
            total.check().map_err(RetryBudgetExceeded::Total)?;
        }
        Ok(())
    }

    pub(crate) fn check_after(
        &self,
        delay: Duration,
    ) -> Result<(), RetryBudgetExceeded> {
        if let Some(total) = &self.total {
            total
                .check_after(delay)
                .map_err(RetryBudgetExceeded::Total)?;
        }
        Ok(())
    }
}
