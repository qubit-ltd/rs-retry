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
///
/// `T` is the successful operation value, and `E` is the application error
/// from the last failed attempt, when one exists.
///
/// # Examples
///
/// ```
/// use qubit_retry::Retry;
/// use qubit_retry::RetryConfig;
/// use qubit_retry::RetryResult;
///
/// let config = RetryConfig::<&str>::builder().build()?;
/// let result: RetryResult<u32, &str> = Retry::new(&config).run(|| Ok(7));
/// assert_eq!(result.unwrap().value(), &7);
/// # Ok::<(), qubit_retry::RetryPolicyError>(())
/// ```
pub type RetryResult<T, E> = Result<RetrySuccess<T>, RetryError<E>>;
