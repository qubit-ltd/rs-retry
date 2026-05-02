/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Adapter that stores the successful value outside the type-erased retry loop.

use crate::error::AttemptFailure;

use super::sync_attempt::SyncAttempt;

/// Adapter that stores the successful value outside the type-erased retry loop.
pub(in crate::executor) struct SyncValueOperation<T, F> {
    /// Wrapped caller operation.
    operation: F,
    /// Successful value produced by the operation.
    value: Option<T>,
}

impl<T, F> SyncValueOperation<T, F> {
    /// Creates a synchronous value-capturing operation adapter.
    ///
    /// # Parameters
    /// - `operation`: Operation to wrap.
    ///
    /// # Returns
    /// A new adapter with no captured value.
    pub(in crate::executor) fn new(operation: F) -> Self {
        Self {
            operation,
            value: None,
        }
    }

    /// Returns the value captured from a successful operation.
    ///
    /// # Returns
    /// The captured value.
    ///
    /// # Panics
    /// Panics only if the retry loop reports success without a successful
    /// operation result, which would indicate an internal logic error.
    pub(in crate::executor) fn into_value(self) -> T {
        self.value
            .expect("retry loop succeeded without an operation value")
    }
}

impl<T, E, F> SyncAttempt<E> for SyncValueOperation<T, F>
where
    F: FnMut() -> Result<T, E>,
{
    /// Calls the wrapped operation and stores successful values.
    ///
    /// # Returns
    /// `Ok(())` after storing a successful value, or an application failure.
    fn call(&mut self) -> Result<(), AttemptFailure<E>> {
        match (self.operation)() {
            Ok(value) => {
                self.value = Some(value);
                Ok(())
            }
            Err(error) => Err(AttemptFailure::Error(error)),
        }
    }
}
