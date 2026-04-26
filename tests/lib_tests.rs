/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
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
