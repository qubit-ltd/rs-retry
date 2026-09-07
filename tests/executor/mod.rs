// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Executor behavior coverage mirrors.

#[cfg(feature = "tokio")]
mod async_boundary_tests;
mod async_cancellation_tests;
#[cfg(feature = "tokio")]
mod async_contract_tests;
mod async_retry_tests;
mod attempt_cancellation_token_tests;
mod before_attempt_tests;
mod blocking_attempt_tests;
mod blocking_value_operation_tests;
mod control_boundary_tests;
mod internal;
mod retry_cancellation_state_tests;
mod retry_cancellation_token_tests;
mod retry_cancelled_tests;
mod retry_tests;
mod sync_boundary_tests;
mod sync_contract_tests;
mod sync_retry_tests;
mod worker_boundary_tests;
mod worker_cancellation_tests;
mod worker_contract_tests;
mod worker_exit_tests;
mod worker_retry_tests;
