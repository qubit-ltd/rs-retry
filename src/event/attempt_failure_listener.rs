/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Attempt failure listener alias.

use qubit_function::{ArcBiConsumer, ArcBiFunction};

use crate::{AttemptFailure, AttemptFailureDecision, RetryContext};

/// Listener invoked when one operation attempt produces a failure.
///
/// The returned decision can override the default retry policy.
pub type AttemptFailureListener<E> =
    ArcBiFunction<AttemptFailure<E>, RetryContext, AttemptFailureDecision>;

/// Listener invoked after a failed attempt has been scheduled for retry.
///
/// The context includes the selected next delay through
/// [`RetryContext::next_delay`]. This listener is observational only and cannot
/// change the retry decision.
pub type RetryScheduledListener<E> = ArcBiConsumer<AttemptFailure<E>, RetryContext>;
