/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Before-attempt listener alias.

use qubit_function::ArcConsumer;

use crate::RetryContext;

/// Listener invoked before every operation attempt.
///
/// The first attempt also triggers this listener.
pub type BeforeAttemptListener = ArcConsumer<RetryContext>;
