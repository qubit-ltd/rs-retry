// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Ordered retry decision rules.

mod internal;
mod retry_decision;
mod retry_fallback;
mod retry_rule;

pub(crate) use internal::RetryRules;
pub use retry_decision::RetryDecision;
pub use retry_fallback::RetryFallback;
pub use retry_rule::RetryRule;
