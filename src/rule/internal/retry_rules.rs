// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal ordered rule collection.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;

use crate::AttemptFailure;
use crate::RetryCallbackFailure;
use crate::RetryCallbackKind;
use crate::RetryCallbackPhase;
use crate::RetryContext;
use crate::internal::retry_panic_from_payload;
use crate::rule::RetryDecision;
use crate::rule::RetryRule;

/// Ordered rules. The first concrete decision wins.
/// # Type Parameters
/// - `E`: Application error borrowed while each rule selects its decision.
pub(crate) struct RetryRules<E> {
    /// Ordered shared rule objects; cloning copies references without cloning
    /// E.
    rules: Arc<[Arc<dyn RetryRule<E>>]>,
}

/// Clones the ordered callback references without cloning the operation error.
impl<E> Clone for RetryRules<E> {
    ///
    /// Clones the ordered rule references without duplicating callback objects.
    ///
    /// # Returns
    /// A collection sharing callbacks but owning its independent vector.
    #[inline]
    fn clone(&self) -> Self {
        Self {
            rules: Arc::clone(&self.rules),
        }
    }
}

impl<E> Default for RetryRules<E> {
    ///
    /// Creates an empty ordered rule collection.
    ///
    /// # Returns
    /// A collection with no registered callbacks and no vector allocation.
    #[inline]
    fn default() -> Self {
        Self { rules: Arc::from([]) }
    }
}

impl<E: 'static> RetryRules<E> {
    pub(crate) fn from_vec(rules: Vec<Arc<dyn RetryRule<E>>>) -> Self {
        Self { rules: rules.into() }
    }

    /// Resolves the first non-default decision.
    ///
    /// Returns the structured failure for the first panicking rule and stops
    /// evaluating later rules.
    ///
    /// # Parameters
    /// - `failure`: Committed attempt failure borrowed by each rule.
    /// - `context`: Current snapshot before scheduling.
    ///
    /// # Returns
    /// The first decisive result, or UseDefault when all rules delegate.
    ///
    /// # Errors
    /// Returns the first captured rule panic with its registration index; later
    /// rules are not invoked.
    pub(crate) fn try_decide(
        &self,
        failure: &AttemptFailure<E>,
        context: &RetryContext,
    ) -> Result<RetryDecision, RetryCallbackFailure> {
        for (index, rule) in self.rules.iter().enumerate() {
            let decision = catch_unwind(AssertUnwindSafe(|| rule.decide(failure, context))).map_err(|payload| {
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
}
