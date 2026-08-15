// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable conversion of unwinding callback payloads.

use std::any::Any;

use crate::RetryPanic;

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
