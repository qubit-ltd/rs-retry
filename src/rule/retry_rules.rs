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
use crate::RetryCallbackFailure;
use crate::RetryCallbackKind;
use crate::RetryCallbackPhase;
use crate::RetryContext;
use crate::observer::RetryDiagnostic;
use crate::observer::retry_panic_from_payload;

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

    /// Resolves the first non-default decision.
    ///
    /// Returns the structured failure for the first panicking rule and stops
    /// evaluating later rules.
    pub(crate) fn try_decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> Result<RetryDecision, RetryCallbackFailure> {
        for (index, rule) in self.rules.iter().enumerate() {
            let decision = std::panic::catch_unwind(AssertUnwindSafe(|| {
                rule.decide(failure, context)
            }))
            .map_err(|payload| {
                RetryCallbackFailure::new(
                    RetryCallbackKind::Rule,
                    index,
                    RetryCallbackPhase::RuleDecision,
                    retry_panic_from_payload(payload),
                )
            })?;
            if !matches!(decision, RetryDecision::UseDefault) {
                return Ok(decision);
            }
        }
        Ok(RetryDecision::UseDefault)
    }

    /// Temporarily adapts the legacy executor contract to [`Self::try_decide`].
    ///
    /// Callback failures are intentionally discarded until the flow controller
    /// consumes the structured result directly.
    pub(crate) fn decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
        _diagnostics: &mut Vec<RetryDiagnostic>,
    ) -> RetryDecision {
        self.try_decide(failure, context)
            .unwrap_or(RetryDecision::UseDefault)
    }
}
