/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
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
