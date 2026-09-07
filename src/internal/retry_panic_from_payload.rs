// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stable conversion of unwinding callback payloads.

use std::any::Any;
use std::mem::forget;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

use crate::RetryPanic;

/// Converts an unwinding callback payload into its stable representation.
///
/// Payload destruction runs inside a second unwind boundary. A destructor
/// panic therefore keeps the `NonString` classification and leaks only the
/// secondary panic payload to prevent recursive unwinding.
///
/// # Parameters
/// - `payload`: Owned unwind payload that must not escape the protected
///   boundary.
///
/// # Returns
/// A borrowed static string, owned String, or NonString classification.
pub(crate) fn retry_panic_from_payload(payload: Box<dyn Any + Send>) -> RetryPanic {
    match catch_unwind(AssertUnwindSafe(|| decode_retry_panic_payload(payload))) {
        Ok(panic) => panic,
        Err(secondary_payload) => {
            forget(secondary_payload);
            RetryPanic::NonString
        }
    }
}

/// Classifies a panic payload and releases non-string payloads inside the
/// caller's unwind boundary.
///
/// # Parameters
/// - `payload`: Owned unwind payload that must not escape the protected
///   boundary.
///
/// # Returns
/// A borrowed static string, owned String, or NonString classification.
///
/// # Panics
/// A non-string payload destructor may panic; callers must use the protected
/// decoder boundary.
fn decode_retry_panic_payload(payload: Box<dyn Any + Send>) -> RetryPanic {
    let payload = match payload.downcast::<&'static str>() {
        Ok(message) => return RetryPanic::StaticStr(*message),
        Err(payload) => payload,
    };
    match payload.downcast::<String>() {
        Ok(message) => RetryPanic::String(*message),
        Err(_) => RetryPanic::NonString,
    }
}
