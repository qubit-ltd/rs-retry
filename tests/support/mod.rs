// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod fixed_retry_random_source;
mod retry_facade_matrix;
mod test_error;

pub(crate) use fixed_retry_random_source::FixedRetryRandomSource;
pub(crate) use retry_facade_matrix::CountingPhaseObserver;
pub(crate) use retry_facade_matrix::ElapsedObserverCallback;
pub(crate) use retry_facade_matrix::ElapsedRuleCallback;
pub(crate) use retry_facade_matrix::ObserverPhaseCounts;
pub(crate) use retry_facade_matrix::PanickingPhaseObserver;
pub(crate) use retry_facade_matrix::assert_callback_panic_elapsed;
pub(crate) use retry_facade_matrix::assert_matrix_abort;
pub(crate) use retry_facade_matrix::assert_matrix_infrastructure;
pub(crate) use retry_facade_matrix::assert_matrix_limit;
pub(crate) use retry_facade_matrix::assert_matrix_observer_panic;
pub(crate) use retry_facade_matrix::assert_matrix_rule_panic;
pub(crate) use retry_facade_matrix::assert_matrix_timeout;
pub(crate) use retry_facade_matrix::callback_elapsed_records;
pub(crate) use retry_facade_matrix::completion_regressing_timer;
pub(crate) use retry_facade_matrix::rule_terminal_regressing_timer;
pub(crate) use test_error::TestError;

mod advancing_observer;
mod retry_once_policy;
mod unit_test_error;

pub(crate) use advancing_observer::AdvancingObserver;
pub(crate) use retry_once_policy::retry_once_policy;
pub(crate) use unit_test_error::UnitTestError;
