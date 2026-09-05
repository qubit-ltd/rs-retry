// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private continuation-budget helper types.

mod retry_budget_state;
mod retry_resource;

pub(crate) use retry_budget_state::RetryBudgetState;
pub(super) use retry_resource::RetryResource;
