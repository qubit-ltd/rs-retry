/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Internal retry listener collection.

use super::{
    AttemptFailureListener, AttemptSuccessListener, BeforeAttemptListener, RetryErrorListener,
    RetryScheduledListener,
};

#[derive(Clone)]
pub(crate) struct RetryListeners<E> {
    /// Callbacks invoked before every attempt.
    pub(crate) before_attempt: Vec<BeforeAttemptListener>,
    /// Callbacks invoked after successful attempts.
    pub(crate) attempt_success: Vec<AttemptSuccessListener>,
    /// Callbacks invoked after a failed attempt.
    pub(crate) failure: Vec<AttemptFailureListener<E>>,
    /// Callbacks invoked after a retry has been scheduled.
    pub(crate) retry_scheduled: Vec<RetryScheduledListener<E>>,
    /// Callbacks invoked when the whole retry flow fails.
    pub(crate) error: Vec<RetryErrorListener<E>>,
}

impl<E> Default for RetryListeners<E> {
    /// Creates an empty listener set.
    ///
    /// # Parameters
    /// This function has no parameters.
    ///
    /// # Returns
    /// A [`RetryListeners`] value with every callback unset.
    ///
    /// # Errors
    /// This function does not return errors.
    #[inline]
    fn default() -> Self {
        Self {
            before_attempt: Vec::new(),
            attempt_success: Vec::new(),
            failure: Vec::new(),
            retry_scheduled: Vec::new(),
            error: Vec::new(),
        }
    }
}
