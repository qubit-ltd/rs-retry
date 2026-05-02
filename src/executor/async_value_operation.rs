/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Adapter that stores async operation success values outside the retry loop.

#[cfg(feature = "tokio")]
use std::future::Future;

#[cfg(feature = "tokio")]
use super::async_attempt::AsyncAttempt;
#[cfg(feature = "tokio")]
use super::async_attempt_future::AsyncAttemptFuture;

use crate::error::AttemptFailure;

/// Adapter that stores async operation success values outside the retry loop.
#[cfg(feature = "tokio")]
pub(in crate::executor) struct AsyncValueOperation<T, F> {
    /// Wrapped caller operation.
    operation: F,
    /// Successful value produced by the operation.
    value: Option<T>,
}

#[cfg(feature = "tokio")]
impl<T, F> AsyncValueOperation<T, F> {
    /// Creates an asynchronous value-capturing operation adapter.
    ///
    /// # Parameters
    /// - `operation`: Operation factory to wrap.
    ///
    /// # Returns
    /// A new adapter with no captured value.
    pub(in crate::executor) fn new(operation: F) -> Self {
        Self {
            operation,
            value: None,
        }
    }

    /// Returns the value captured from a successful async operation.
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

#[cfg(feature = "tokio")]
impl<T, E, F, Fut> AsyncAttempt<E> for AsyncValueOperation<T, F>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    /// Calls the wrapped async operation and stores successful values.
    ///
    /// # Returns
    /// A future resolving to `Ok(())` after storing a successful value, or an
    /// application failure.
    fn call(&mut self) -> AsyncAttemptFuture<'_, E> {
        Box::pin(async move {
            match (self.operation)().await {
                Ok(value) => {
                    self.value = Some(value);
                    Ok(())
                }
                Err(error) => Err(AttemptFailure::Error(error)),
            }
        })
    }
}
