/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry listener type aliases.
//!
//! Listener callbacks are shared through `rs-function` functors so cloned
//! executors invoke the same callback set and can compose listeners with
//! function combinators.

use qubit_function::{ArcBiConsumer, ArcConsumer};

use crate::RetryAttemptFailure;

use super::{RetryAbortContext, RetryContext, RetryFailureContext, RetrySuccessContext};

/// Listener invoked before sleeping for a retry.
///
/// The callback receives retry metadata and the triggering failure separately.
pub type RetryListener<E> = ArcBiConsumer<RetryContext, RetryAttemptFailure<E>>;

/// Listener invoked when the operation eventually succeeds.
///
/// The callback receives a borrowed [`RetrySuccessContext`] and is invoked exactly
/// once for a successful executor execution.
pub type RetrySuccessListener = ArcConsumer<RetrySuccessContext>;

/// Listener invoked when retry limits are exhausted.
///
/// The callback receives failure metadata plus an optional final failure
/// payload (`None` means stopped before the first attempt).
pub type RetryFailureListener<E> =
    ArcBiConsumer<RetryFailureContext, Option<RetryAttemptFailure<E>>>;

/// Listener invoked when the retry decider aborts retrying.
///
/// The callback receives abort metadata and the triggering failure separately.
pub type RetryAbortListener<E> = ArcBiConsumer<RetryAbortContext, RetryAttemptFailure<E>>;

#[derive(Clone)]
pub(crate) struct RetryListeners<E> {
    /// Callbacks invoked before sleeping for a retry.
    pub(crate) retry: Vec<RetryListener<E>>,
    /// Callbacks invoked when the operation eventually succeeds.
    pub(crate) success: Vec<RetrySuccessListener>,
    /// Callbacks invoked when retry limits are exhausted.
    pub(crate) failure: Vec<RetryFailureListener<E>>,
    /// Callbacks invoked when the retry decider aborts retrying.
    pub(crate) abort: Vec<RetryAbortListener<E>>,
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
            retry: Vec::new(),
            success: Vec::new(),
            failure: Vec::new(),
            abort: Vec::new(),
        }
    }
}
