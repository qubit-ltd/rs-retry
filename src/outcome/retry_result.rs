// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Result alias returned by retry executors.

use super::RetryError;
use super::RetrySuccess;

/// Result returned by a retry executor after success or terminal failure.
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;
