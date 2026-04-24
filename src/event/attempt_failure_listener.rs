/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Attempt failure listener alias.

use qubit_function::ArcBiFunction;

use crate::{AttemptFailure, AttemptFailureDecision, RetryContext};

/// Listener invoked when one operation attempt produces a failure.
///
/// The returned decision can override the default retry policy.
pub type AttemptFailureListener<E> =
    ArcBiFunction<AttemptFailure<E>, RetryContext, AttemptFailureDecision>;
