// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal ordered rule collection.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use super::RetryDecision;
use super::RetryRule;
use crate::AttemptFailure;
use crate::RetryContext;
use crate::observer::RetryDiagnostic;
use crate::observer::RetryDiagnosticKind;

/// Ordered rules. The first concrete decision wins.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct RetryRules<E> {
    rules: Vec<Arc<dyn RetryRule<E>>>,
}

impl<E> Default for RetryRules<E> {
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}

impl<E: 'static> RetryRules<E> {
    /// Appends one rule in evaluation order.
    pub(crate) fn push<R>(&mut self, rule: R)
    where
        R: RetryRule<E>,
    {
        self.rules.push(Arc::new(rule));
    }

    /// Resolves the first non-default decision and collects diagnostics.
    pub(crate) fn decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
        diagnostics: &mut Vec<RetryDiagnostic>,
    ) -> RetryDecision {
        for (index, rule) in self.rules.iter().enumerate() {
            let decision = std::panic::catch_unwind(AssertUnwindSafe(|| {
                rule.decide(failure, context)
            }))
            .unwrap_or_else(|_| {
                diagnostics.push(RetryDiagnostic::new(
                    RetryDiagnosticKind::RulePanicked,
                    index,
                ));
                RetryDecision::UseDefault
            });
            if !matches!(decision, RetryDecision::UseDefault) {
                return decision;
            }
        }
        RetryDecision::UseDefault
    }
}
