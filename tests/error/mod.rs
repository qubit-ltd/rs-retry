// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Error behavior coverage mirrors.

mod attempt_failure_contract_tests;
mod attempt_failure_tests;
mod failure_components_tests;
mod retry_callback_failure_tests;
mod retry_callback_kind_tests;
mod retry_callback_phase_tests;
mod retry_cancellation_phase_tests;
mod retry_error_tests;
#[cfg(feature = "worker")]
mod retry_failure_tests;
#[cfg(feature = "worker")]
mod retry_infrastructure_failure_tests;
mod retry_limit_kind_tests;
mod retry_panic_tests;
mod retry_policy_error_tests;
mod retry_timeout_scope_tests;
mod terminal_accessors_tests;
#[cfg(feature = "worker")]
mod worker_stop_trigger_tests;
