/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Boxed future used by a value-erased async attempt.

#[cfg(feature = "tokio")]
use std::future::Future;
#[cfg(feature = "tokio")]
use std::pin::Pin;

use crate::error::AttemptFailure;

/// Boxed future returned by a value-erased async attempt.
#[cfg(feature = "tokio")]
pub(in crate::executor) type AsyncAttemptFuture<'a, E> =
    Pin<Box<dyn Future<Output = Result<(), AttemptFailure<E>>> + 'a>>;
