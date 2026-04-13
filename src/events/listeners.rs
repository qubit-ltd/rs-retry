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
//! Listener callbacks are shared with [`Arc`] so cloned executors invoke the
//! same callback set.

use std::sync::Arc;

use qubit_function::ArcConsumer;

use super::{AbortEvent, FailureEvent, RetryEvent, SuccessEvent};

/// Listener invoked before sleeping for a retry.
///
/// The callback receives a borrowed [`RetryEvent`] and must be safe to share
/// across threads because executors are cloneable.
pub type RetryListener<E> = Arc<dyn for<'a> Fn(&RetryEvent<'a, E>) + Send + Sync + 'static>;

/// Listener invoked when the operation eventually succeeds.
///
/// The callback receives a borrowed [`SuccessEvent`] and is invoked exactly
/// once for a successful executor execution.
pub type SuccessListener = Arc<dyn Fn(&SuccessEvent) + Send + Sync + 'static>;

/// Listener invoked when retry limits are exhausted.
///
/// The callback receives a borrowed [`FailureEvent`] when attempts or elapsed
/// budget stops retrying.
pub type FailureListener<E> = Arc<dyn for<'a> Fn(&FailureEvent<'a, E>) + Send + Sync + 'static>;

/// Listener invoked when the classifier aborts retrying.
///
/// The callback receives a borrowed [`AbortEvent`] when the classifier returns
/// [`crate::RetryDecision::Abort`].
pub type AbortListener<E> = Arc<dyn for<'a> Fn(&AbortEvent<'a, E>) + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct RetryListeners<E> {
    /// Optional callback invoked before sleeping for a retry.
    pub(crate) retry: Option<RetryListener<E>>,
    /// Optional callback invoked when the operation eventually succeeds.
    pub(crate) success: Option<ArcConsumer<SuccessEvent>>,
    /// Optional callback invoked when retry limits are exhausted.
    pub(crate) failure: Option<FailureListener<E>>,
    /// Optional callback invoked when the classifier aborts retrying.
    pub(crate) abort: Option<AbortListener<E>>,
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
            retry: None,
            success: None,
            failure: None,
            abort: None,
        }
    }
}
