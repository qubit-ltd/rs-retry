// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Adapter that stores worker operation success values outside the retry loop.

use std::sync::Mutex;
use std::sync::PoisonError;

use super::blocking_attempt::BlockingAttempt;
use crate::AttemptFailure;
use crate::executor::AttemptCancellationToken;

/// Adapter that stores worker operation success values outside the retry loop.
/// # Type Parameters
/// - `T`: Successful value moved into the protected result slot.
/// - `F`: Shared operation callable from successive worker threads.
pub(in crate::executor) struct BlockingValueOperation<T, F> {
    /// Wrapped caller operation.
    operation: F,
    /// Successful value produced by the operation.
    value: Mutex<Option<T>>,
}

impl<T, F> BlockingValueOperation<T, F> {
    /// Creates a blocking worker value-capturing operation adapter.
    ///
    /// # Parameters
    /// - `operation`: Operation to wrap.
    ///
    /// # Returns
    /// A new adapter with no captured value.
    #[inline(always)]
    #[must_use = "use the prepared value or inspect the result"]
    pub(in crate::executor) fn new(operation: F) -> Self {
        Self {
            operation,
            value: Mutex::new(None),
        }
    }

    /// Takes the value captured from a successful operation.
    ///
    /// # Returns
    /// The captured value.
    ///
    /// # Panics
    /// Panics only if the retry loop reports success without a successful
    /// operation result, which would indicate an internal logic error.
    #[inline(always)]
    pub(in crate::executor) fn take_value(&self) -> T {
        let mut value =
            self.value.lock().unwrap_or_else(PoisonError::into_inner);
        value
            .take()
            .expect("retry loop succeeded without an operation value")
    }
}

impl<T, E, F> BlockingAttempt<E> for BlockingValueOperation<T, F>
where
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static,
{
    /// Calls the wrapped operation and stores successful values.
    ///
    /// # Parameters
    /// - `token`: Cooperative cancellation token for this attempt.
    ///
    /// # Returns
    /// `Ok(())` after storing a successful value, or an application failure.
    ///
    /// # Errors
    /// Returns the original application failure without cloning it.
    ///
    /// # Panics
    /// The operation may unwind; the worker executor owns the panic capture
    /// boundary.
    #[inline]
    fn call(
        &self,
        token: AttemptCancellationToken,
    ) -> Result<(), AttemptFailure<E>> {
        match (self.operation)(token) {
            Ok(result) => {
                let mut value =
                    self.value.lock().unwrap_or_else(PoisonError::into_inner);
                *value = Some(result);
                Ok(())
            }
            Err(error) => Err(AttemptFailure::Error(error)),
        }
    }
}
