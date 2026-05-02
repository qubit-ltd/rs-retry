/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Message sent from one blocking attempt worker to the retry executor.

use crate::error::AttemptPanic;

/// Message sent from one blocking attempt worker to the retry executor.
pub(in crate::executor) enum BlockingAttemptMessage<T, E> {
    /// Operation returned normally.
    Result(Result<T, E>),
    /// Operation panicked before timeout.
    Panic(AttemptPanic),
}
