// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_function::BiFunction;
use qubit_retry::AttemptFailure;
use qubit_retry::AttemptFailureDecision;
use qubit_retry::RetryContext;

use super::TestError;

/// Test failure listener that aborts on fatal application errors.
pub(crate) struct AbortFatal;

impl BiFunction<AttemptFailure<TestError>, RetryContext, AttemptFailureDecision>
    for AbortFatal
{
    /// Applies the test decider.
    ///
    /// # Parameters
    /// - `failure`: Failure being handled.
    /// - `_context`: Retry context.
    ///
    /// # Returns
    /// Abort for fatal errors, otherwise use the default policy.
    fn apply(
        &self,
        failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) -> AttemptFailureDecision {
        match failure {
            AttemptFailure::Error(TestError("fatal")) => {
                AttemptFailureDecision::Abort
            }
            _ => AttemptFailureDecision::UseDefault,
        }
    }
}
