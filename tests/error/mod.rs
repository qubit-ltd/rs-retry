/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

mod attempt_executor_error_tests;
mod attempt_failure_tests;
mod attempt_panic_tests;
mod retry_config_error_basic_tests;
#[cfg(feature = "config")]
mod retry_config_error_tests;
mod retry_error_tests;
