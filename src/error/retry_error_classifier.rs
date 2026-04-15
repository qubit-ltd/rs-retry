/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use crate::event::{RetryAttemptContext, RetryDecision};
use qubit_function::ArcBiFunction;

/// Classifies an application error as retryable or non-retryable.
///
/// The classifier receives the original application error and the current
/// attempt context. Returning [`RetryDecision::Retry`] allows the executor to
/// continue if attempt and elapsed-time limits still permit another try;
/// returning [`RetryDecision::Abort`] stops immediately with
/// [`crate::RetryError::Aborted`].
///
/// The classifier is stored as an [`ArcBiFunction`] so cloned
/// [`crate::RetryExecutor`] instances can share it safely.
pub type RetryErrorClassifier<E> = ArcBiFunction<E, RetryAttemptContext, RetryDecision>;
