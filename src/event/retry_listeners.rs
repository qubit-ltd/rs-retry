/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Internal retry listener collection.

use super::{
    AttemptFailureListener, AttemptSuccessListener, BeforeAttemptListener, RetryErrorListener,
};

#[derive(Clone)]
pub(crate) struct RetryListeners<E> {
    /// Callbacks invoked before every attempt.
    pub(crate) before_attempt: Vec<BeforeAttemptListener>,
    /// Callbacks invoked after successful attempts.
    pub(crate) attempt_success: Vec<AttemptSuccessListener>,
    /// Callbacks invoked after a failed attempt.
    pub(crate) failure: Vec<AttemptFailureListener<E>>,
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
            error: Vec::new(),
        }
    }
}
