// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared attempt lifecycle transitions for retry runners.

use std::time::Duration;

use qubit_clock::MonotonicInstant;

use crate::RetryContext;
use crate::RetryError;
use crate::RetryOptions;
use crate::event::RetryEvents;
use crate::executor::retry_flow_state::RetryFlowState;
use crate::options::EffectiveAttemptTimeout;

/// Prepares an attempt whose running operation cannot be interrupted.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits used for checks and context construction.
/// * `events` - Listener dispatcher invoked before the attempt is committed.
///
/// # Returns
///
/// A timeout-free attempt descriptor after the attempt is committed.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error when the budget is exhausted before
/// the operation enters execution.
#[allow(clippy::result_large_err)]
pub(in crate::executor) fn prepare_same_thread_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
) -> Result<EffectiveAttemptTimeout, RetryError<E>> {
    prepare_attempt(state, options, events, |_| EffectiveAttemptTimeout::none())
}

/// Prepares an async or worker attempt with the shortest effective timeout.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits and configured timeout.
/// * `events` - Listener dispatcher invoked before the attempt is committed.
///
/// # Returns
///
/// The effective timeout recomputed after pre-attempt listeners.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error when no budget remains before the
/// operation enters execution.
#[allow(clippy::result_large_err)]
pub(in crate::executor) fn prepare_timed_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
) -> Result<EffectiveAttemptTimeout, RetryError<E>> {
    prepare_attempt(state, options, events, |state| {
        options.effective_attempt_timeout(
            state.operation_elapsed(),
            state.total_elapsed(),
        )
    })
}

/// Records one completed attempt and builds its context.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits copied into the context.
/// * `attempt_start` - Monotonic instant captured immediately before execution.
/// * `attempt_timeout` - Effective timeout used for this attempt.
///
/// # Returns
///
/// The completed-attempt context with updated elapsed durations.
pub(in crate::executor) fn complete_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    attempt_start: MonotonicInstant,
    attempt_timeout: EffectiveAttemptTimeout,
) -> RetryContext {
    let attempt_elapsed = state.elapsed_since(attempt_start);
    state.add_operation_elapsed(attempt_elapsed);
    state.context(options, attempt_elapsed, attempt_timeout)
}

/// Runs pre-attempt checks and commits the next attempt.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits used for checks and context construction.
/// * `events` - Listener dispatcher invoked before commitment.
/// * `effective_timeout` - Resolver evaluated at each budget control point.
///
/// # Returns
///
/// The final effective timeout after the listener and budget recheck.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error before committing the attempt when
/// either pre-attempt check finds an exhausted budget.
#[allow(clippy::result_large_err)]
fn prepare_attempt<E, F>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
    effective_timeout: F,
) -> Result<EffectiveAttemptTimeout, RetryError<E>>
where
    F: Fn(&RetryFlowState<'_, E>) -> EffectiveAttemptTimeout,
{
    let attempt_timeout = effective_timeout(state);
    if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
        return Err(error);
    }

    let attempt_timeout = effective_timeout(state);
    let context =
        state.next_attempt_context(options, Duration::ZERO, attempt_timeout);
    events.before_attempt(&context);

    let attempt_timeout = effective_timeout(state);
    if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
        return Err(error);
    }
    state.start_next_attempt();
    Ok(attempt_timeout)
}
