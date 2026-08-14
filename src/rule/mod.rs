// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Ordered retry decision rules.

mod retry_decision;
mod retry_rule;
mod retry_rules;

pub use retry_decision::RetryDecision;
pub use retry_rule::RetryRule;
pub(crate) use retry_rules::RetryRules;
