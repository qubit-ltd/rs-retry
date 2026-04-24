/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry-after hint extractor alias.

use std::time::Duration;

use qubit_function::ArcBiFunction;

use crate::{AttemptFailure, RetryContext};

/// Extracts an optional retry-after delay from an attempt failure.
pub type RetryAfterHint<E> = ArcBiFunction<AttemptFailure<E>, RetryContext, Option<Duration>>;
