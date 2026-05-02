/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/

mod attempt_timeout_option_tests;
mod attempt_timeout_policy_tests;
#[cfg(feature = "config")]
mod retry_config_values_tests;
mod retry_delay_duration_format_tests;
mod retry_delay_tests;
mod retry_jitter_tests;
mod retry_options_basic_tests;
#[cfg(feature = "config")]
mod retry_options_tests;
