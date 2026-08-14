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

#[cfg(test)]
mod tests {
    use std::panic::panic_any;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use super::RetryRules;
    use crate::AttemptFailure;
    use crate::RetryCallbackKind;
    use crate::RetryCallbackPhase;
    use crate::RetryContext;
    use crate::RetryDecision;
    use crate::RetryPanic;

    #[derive(Clone, Copy, Debug)]
    enum PanicPayload {
        StaticStr,
        String,
        NonString,
    }

    impl PanicPayload {
        /// Panics with the payload represented by this test case.
        fn raise(self) -> ! {
            match self {
                Self::StaticStr => panic!("static rule panic"),
                Self::String => panic_any(String::from("owned rule panic")),
                Self::NonString => panic_any(23_u32),
            }
        }

        /// Returns the stable payload expected from this test case.
        fn expected(self) -> RetryPanic {
            match self {
                Self::StaticStr => RetryPanic::StaticStr("static rule panic"),
                Self::String => {
                    RetryPanic::String(String::from("owned rule panic"))
                }
                Self::NonString => RetryPanic::NonString,
            }
        }
    }

    /// Verifies exact rule panic attribution for every supported payload.
    #[test]
    fn test_try_decide_reports_exact_callback_failure() {
        for payload in [
            PanicPayload::StaticStr,
            PanicPayload::String,
            PanicPayload::NonString,
        ] {
            let panicking_calls = Arc::new(AtomicUsize::new(0));
            let later_calls = Arc::new(AtomicUsize::new(0));
            let mut rules = RetryRules::<&'static str>::default();
            rules.push(|_: &AttemptFailure<&str>, _: &RetryContext| {
                RetryDecision::UseDefault
            });
            rules.push({
                let panicking_calls = Arc::clone(&panicking_calls);
                move |_: &AttemptFailure<&str>, _: &RetryContext| {
                    panicking_calls.fetch_add(1, Ordering::SeqCst);
                    payload.raise();
                }
            });
            rules.push({
                let later_calls = Arc::clone(&later_calls);
                move |_: &AttemptFailure<&str>, _: &RetryContext| {
                    later_calls.fetch_add(1, Ordering::SeqCst);
                    RetryDecision::Abort
                }
            });

            let failure = rules
                .try_decide(
                    &AttemptFailure::Error("operation failed"),
                    &RetryContext::new(1, 2),
                )
                .expect_err("the second rule should panic");

            assert_eq!(failure.callback(), RetryCallbackKind::Rule);
            assert_eq!(failure.index(), 1);
            assert_eq!(failure.phase(), RetryCallbackPhase::RuleDecision);
            assert_eq!(failure.panic(), &payload.expected());
            assert_eq!(panicking_calls.load(Ordering::SeqCst), 1);
            assert_eq!(later_calls.load(Ordering::SeqCst), 0);
        }
    }
}
