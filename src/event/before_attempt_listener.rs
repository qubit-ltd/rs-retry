/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Before-attempt listener alias.

use qubit_function::ArcConsumer;

use crate::RetryContext;

/// Listener invoked before every operation attempt.
///
/// The first attempt also triggers this listener.
pub type BeforeAttemptListener = ArcConsumer<RetryContext>;
