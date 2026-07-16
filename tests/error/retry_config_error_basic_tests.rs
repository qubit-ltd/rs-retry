// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::error::Error;

use qubit_argument::{
    StringArgument,
    require_that,
};
use qubit_retry::RetryConfigError;

/// Verifies basic configuration error accessors and empty-path formatting.
#[test]
fn test_retry_config_error_accessors_and_empty_path_display() {
    let empty_path = RetryConfigError::invalid_value("", "missing value");
    assert_eq!(empty_path.path(), "");
    assert_eq!(empty_path.message(), "missing value");
    assert_eq!(
        empty_path.to_string(),
        "invalid retry configuration: missing value"
    );

    let keyed = RetryConfigError::invalid_value(
        "retry.max_attempts",
        "must be positive",
    );
    assert_eq!(keyed.path(), "retry.max_attempts");
    assert_eq!(keyed.message(), "must be positive");
    assert_eq!(
        keyed.to_string(),
        "invalid retry configuration at 'retry.max_attempts': must be positive"
    );
}

/// Verifies argument validation errors preserve their path and message.
#[test]
fn test_retry_config_error_from_argument_error() {
    let argument_error = require_that(
        0_u32,
        "max_attempts",
        |value| *value > 0,
        "positive",
        "max_attempts must be greater than zero",
    )
    .expect_err("zero attempts should be rejected");

    let error = RetryConfigError::from(argument_error);

    assert_eq!(error.path(), "max_attempts");
    assert_eq!(error.message(), "max_attempts must be greater than zero");
    assert_eq!(
        error.to_string(),
        "invalid retry configuration at 'max_attempts': max_attempts must be greater than zero"
    );
    assert!(error.source().is_none());
}

/// Verifies standard argument diagnostics render their path only once.
#[test]
fn test_retry_config_error_from_standard_argument_error() {
    let argument_error = "   "
        .require_non_blank("retry.name")
        .expect_err("blank names should be rejected");

    let error = RetryConfigError::from(argument_error);
    let diagnostic = error.to_string();

    assert_eq!(error.path(), "retry.name");
    assert_eq!(error.message(), "argument 'retry.name' must not be blank");
    assert_eq!(
        error
            .source()
            .expect("structured argument source should be retained")
            .to_string(),
        "argument 'retry.name' must not be blank"
    );
    assert_eq!(diagnostic.matches("retry.name").count(), 1);
    assert_eq!(
        diagnostic,
        "invalid retry configuration: argument 'retry.name' must not be blank"
    );
}
