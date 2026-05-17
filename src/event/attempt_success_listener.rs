/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Attempt-success listener alias.

use qubit_function::ArcConsumer;

use crate::RetryContext;

/// Listener invoked when an operation attempt succeeds.
///
/// The operation result value is returned by `run` or `run_async`; it is not
/// passed to policy-level listeners because each run call chooses its own
/// success type.
pub type AttemptSuccessListener = ArcConsumer<RetryContext>;
