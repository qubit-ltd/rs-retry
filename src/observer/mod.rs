// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Retry lifecycle observation.

mod retry_diagnostic;
mod retry_observer;
mod retry_observers;
mod retry_outcome_kind;

pub use retry_diagnostic::RetryDiagnostic;
pub use retry_diagnostic::RetryDiagnosticKind;
pub use retry_observer::RetryObserver;
#[allow(unused_imports)]
pub(crate) use retry_observers::RetryObservers;
pub use retry_outcome_kind::RetryOutcomeKind;

pub use crate::event::RetryContext;
