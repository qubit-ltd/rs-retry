/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry error listener alias.

use qubit_function::ArcBiConsumer;

use crate::{RetryContext, RetryError};

/// Listener invoked when the whole retry flow returns an error.
///
/// This listener is observational only and cannot resume a stopped retry flow.
pub type RetryErrorListener<E> = ArcBiConsumer<RetryError<E>, RetryContext>;
