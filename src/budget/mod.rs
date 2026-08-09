// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Continuation-budget state for retry flows.

mod retry_attempt;
mod retry_budget;
mod retry_budget_error;
mod retry_budget_exhausted;
mod retry_budget_snapshot;

pub use retry_attempt::RetryAttempt;
pub use retry_budget::RetryBudget;
pub use retry_budget_error::RetryBudgetError;
pub use retry_budget_exhausted::RetryBudgetExhausted;
pub use retry_budget_snapshot::RetryBudgetSnapshot;
