// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry lifecycle observation.

use std::any::Any;

use crate::RetryPanic;

mod retry_diagnostic;
mod retry_diagnostic_kind;
mod retry_observer;
mod retry_observers;
mod retry_outcome_kind;

pub use retry_diagnostic::RetryDiagnostic;
pub use retry_diagnostic_kind::RetryDiagnosticKind;
pub use retry_observer::RetryObserver;
pub(crate) use retry_observers::RetryObservers;
pub use retry_outcome_kind::RetryOutcomeKind;

pub use crate::event::RetryContext;

/// Converts an unwinding callback payload into its stable representation.
pub(crate) fn retry_panic_from_payload(
    payload: Box<dyn Any + Send>,
) -> RetryPanic {
    let payload = match payload.downcast::<&'static str>() {
        Ok(message) => return RetryPanic::StaticStr(*message),
        Err(payload) => payload,
    };
    match payload.downcast::<String>() {
        Ok(message) => RetryPanic::String(*message),
        Err(_) => RetryPanic::NonString,
    }
}
