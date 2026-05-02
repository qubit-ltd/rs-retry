/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Integration tests for `qubit-retry`.

mod error;
mod event;
mod executor;
mod options;
mod support;

#[cfg(coverage)]
#[path = "coverage/coverage_support_tests.rs"]
mod coverage_support_tests;
