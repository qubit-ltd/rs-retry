/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry error listener alias.

use qubit_function::ArcBiConsumer;

use crate::{RetryContext, RetryError};

/// Listener invoked when the whole retry flow returns an error.
///
/// This listener is observational only and cannot resume a stopped retry flow.
pub type RetryErrorListener<E> = ArcBiConsumer<RetryError<E>, RetryContext>;
